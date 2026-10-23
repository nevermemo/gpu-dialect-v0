//! Runtime access to the external Slang compiler.
//!
//! The procedural macro emits portable Slang source. Host backends call the
//! compiler only when they need a concrete target such as SPIR-V or WGSL.

use std::{
    error, fmt, fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{KernelDescriptor, SourceOrigin};
use source_map::{SourceMap, SourceMapSegment};

mod source_map;

static NEXT_INVOCATION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Spirv,
    Wgsl,
}

impl Target {
    const fn slang_name(self) -> &'static str {
        match self {
            Self::Spirv => "spirv",
            Self::Wgsl => "wgsl",
        }
    }

    const fn extension(self) -> &'static str {
        match self {
            Self::Spirv => "spv",
            Self::Wgsl => "wgsl",
        }
    }
}

#[derive(Debug)]
pub enum Error {
    CompilerNotFound,
    Io(std::io::Error),
    CompilationFailed { target: Target, diagnostics: String },
    InvalidSpirvLength(usize),
    InvalidSpirv(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CompilerNotFound => write!(
                formatter,
                "could not launch `slangc`; install Slang and make slangc available on PATH"
            ),
            Self::Io(error) => write!(formatter, "Slang artifact I/O failed: {error}"),
            Self::CompilationFailed {
                target,
                diagnostics,
            } => write!(
                formatter,
                "Slang failed to compile the {:?} target:\n{}",
                target,
                diagnostics.trim()
            ),
            Self::InvalidSpirvLength(bytes) => {
                write!(formatter, "Slang returned a {bytes}-byte SPIR-V artifact")
            }
            Self::InvalidSpirv(reason) => {
                write!(formatter, "Slang returned invalid SPIR-V: {reason}")
            }
        }
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

pub fn compile(kernel: &KernelDescriptor, target: Target) -> Result<Vec<u8>, Error> {
    compile_with_origin(
        kernel.slang_source,
        kernel.entry_point,
        &kernel.source,
        target,
    )
}

pub fn compile_spirv(kernel: &KernelDescriptor) -> Result<Vec<u32>, Error> {
    let bytes = compile(kernel, Target::Spirv)?;
    decode_spirv(&bytes)
}

pub(crate) fn decode_spirv(bytes: &[u8]) -> Result<Vec<u32>, Error> {
    if !bytes.len().is_multiple_of(size_of::<u32>()) {
        return Err(Error::InvalidSpirvLength(bytes.len()));
    }
    // The length check above leaves no remainder.
    let (chunks, remainder) = bytes.as_chunks::<{ size_of::<u32>() }>();
    debug_assert!(remainder.is_empty());
    let words: Vec<_> = chunks.iter().copied().map(u32::from_le_bytes).collect();
    crate::spirv::validate_structure(&words).map_err(Error::InvalidSpirv)?;
    Ok(words)
}

pub fn compile_wgsl(kernel: &KernelDescriptor) -> Result<String, Error> {
    let bytes = compile(kernel, Target::Wgsl)?;
    String::from_utf8(bytes)
        .map_err(|error| Error::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, error)))
}

pub fn compile_source(source: &str, entry: &str, target: Target) -> Result<Vec<u8>, Error> {
    compile_with_origin(source, entry, &SourceOrigin::UNKNOWN, target)
}

fn compile_with_origin(
    source: &str,
    entry: &str,
    origin: &SourceOrigin,
    target: Target,
) -> Result<Vec<u8>, Error> {
    compile_in_directory(TemporaryDirectory::create()?, source, entry, origin, target)
}

fn compile_in_directory(
    directory: TemporaryDirectory,
    source: &str,
    entry: &str,
    origin: &SourceOrigin,
    target: Target,
) -> Result<Vec<u8>, Error> {
    // The guard removes the directory when this returns, on every path.
    compile_at(&directory.0, source, entry, origin, target)
}

/// Compile inside `directory`, which the caller owns and removes.
fn compile_at(
    directory: &Path,
    source: &str,
    entry: &str,
    origin: &SourceOrigin,
    target: Target,
) -> Result<Vec<u8>, Error> {
    let source_path = directory.join("kernel.slang");
    let map_path = directory.join("kernel.map.json");
    let output_path = directory.join(format!("kernel.{}", target.extension()));
    let source_map = SourceMap::from_slang(source);
    fs::write(&source_path, source)?;
    fs::write(&map_path, source_map.json(origin))?;

    let mut command = Command::new("slangc");
    command.arg(&source_path).args([
        "-target",
        target.slang_name(),
        "-entry",
        entry,
        "-stage",
        "compute",
    ]);
    if target == Target::Spirv {
        command.arg("-fvk-use-entrypoint-name");
    }
    let result = command.arg("-o").arg(&output_path).output();
    let output = match result {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(Error::CompilerNotFound);
        }
        Err(error) => {
            return Err(Error::Io(error));
        }
    };
    if !output.status.success() {
        let diagnostics = format!(
            "{}\n{}\ncompiler exit: {}",
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout),
            output.status
        );
        let diagnostics = annotate_diagnostics(origin, source, &source_map, &diagnostics);
        return Err(Error::CompilationFailed {
            target,
            diagnostics,
        });
    }
    let artifact = fs::read(&output_path)?;
    Ok(artifact)
}

/// Append the originating Rust kernel and construct (DX19), and the Rust source
/// location when the descriptor carries a line table, to a Slang failure.
/// Shared by every path that compiles a kernel, so none reports less than the
/// others. A diagnostic with no usable error location is returned unchanged.
pub(crate) fn annotate_diagnostics(
    origin: &SourceOrigin,
    slang_source: &str,
    source_map: &SourceMap,
    diagnostics: &str,
) -> String {
    let segment = map_slang_diagnostic(source_map, slang_source, diagnostics);
    // The Rust line is independent of the kernel segment: a helper or struct above
    // the first `// @rust kernel:` marker has a line record but no construct.
    let rust_line = parse_slang_line(diagnostics)
        .filter(|line| *line <= slang_source.lines().count())
        .and_then(|line| u32::try_from(line).ok())
        .and_then(|line| origin.rust_line_at(line))
        .filter(|_| !origin.file.is_empty());
    let mut annotated = diagnostics.to_owned();
    if let Some(segment) = segment {
        annotated.push_str(&format!(
            "\noriginating Rust kernel: {}\noriginating Rust construct: {} #{}",
            segment.kernel, segment.construct, segment.ordinal
        ));
    }
    if let Some(rust_line) = rust_line {
        annotated.push_str(&format!(
            "\noriginating Rust source: {}:{rust_line}",
            origin.file
        ));
    }
    annotated
}

/// [`annotate_diagnostics`] for a kernel that is compiled without the sidecar
/// map, such as the reflection helper path.
pub(crate) fn annotate_kernel_diagnostics(kernel: &KernelDescriptor, diagnostics: &str) -> String {
    let map = SourceMap::from_slang(kernel.slang_source);
    annotate_diagnostics(&kernel.source, kernel.slang_source, &map, diagnostics)
}

/// Map a `slangc` diagnostic to a generated construct segment. The sidecar
/// `kernel.map.json` is written for inspection and is never parsed back; the
/// in-memory map built from the same source does the lookup. A line outside
/// every construct segment maps to the nearest preceding kernel marker, so a
/// missing or unreadable segment degrades to kernel attribution.
fn map_slang_diagnostic(
    source_map: &SourceMap,
    slang_source: &str,
    diagnostic: &str,
) -> Option<SourceMapSegment> {
    let target_line = parse_slang_line(diagnostic)?;
    if target_line > slang_source.lines().count() {
        return None;
    }
    source_map.origin_at(target_line)
}

/// Select an error in this compilation unit, not an earlier warning or a filename
/// that merely ends with `kernel.slang`. Both `(line)` and `(line,column)` occur
/// in compiler diagnostics. Unrecognized locations remain unmapped.
fn parse_slang_line(diagnostic: &str) -> Option<usize> {
    let mut error_header = false;
    diagnostic.lines().find_map(|diagnostic| {
        let diagnostic = diagnostic.trim();
        // Current Slang prints an error header followed by a separate arrow
        // location. Track severity so an earlier warning cannot claim the error.
        if diagnostic.starts_with("error[") || diagnostic.starts_with("error:") {
            error_header = true;
        } else if ["warning[", "warning:", "note:", "help:"]
            .iter()
            .any(|prefix| diagnostic.starts_with(prefix))
        {
            error_header = false;
        }
        if error_header && let Some(location) = diagnostic.strip_prefix("--> ") {
            let (path_line, column) = location.rsplit_once(':')?;
            let (path, line) = path_line.rsplit_once(':')?;
            if path.rsplit(['/', '\\']).next()? != "kernel.slang" {
                return None;
            }
            positive_coordinate(column)?;
            return positive_coordinate(line);
        }
        let (location, message) = diagnostic.trim().split_once("):")?;
        let message = message.trim_start();
        let message = message.strip_prefix("fatal ").unwrap_or(message);
        if !message.starts_with("error:") && !message.starts_with("error ") {
            return None;
        }
        let (path, position) = location.rsplit_once('(')?;
        if path.rsplit(['/', '\\']).next()? != "kernel.slang" {
            return None;
        }
        let mut coordinates = position.split(',');
        let line = positive_coordinate(coordinates.next()?)?;
        if let Some(column) = coordinates.next() {
            positive_coordinate(column)?;
        }
        if coordinates.next().is_some() {
            return None;
        }
        Some(line)
    })
}

fn positive_coordinate(value: &str) -> Option<usize> {
    let value = value.trim();
    if !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse().ok().filter(|value| *value > 0)
}

pub struct TemporaryDirectory(PathBuf);

impl TemporaryDirectory {
    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn create() -> Result<Self, std::io::Error> {
        for _ in 0..128 {
            let id = NEXT_INVOCATION.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!("gust-slang-{}-{id}", std::process::id()));
            if let Some(directory) = Self::try_create(path)? {
                return Ok(directory);
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "could not reserve a Slang temporary directory after 128 attempts",
        ))
    }

    fn try_create(path: PathBuf) -> Result<Option<Self>, std::io::Error> {
        match fs::create_dir(&path) {
            Ok(()) => Ok(Some(Self(path))),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(None),
            Err(error) => Err(error),
        }
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        // Only this exclusively created child is owned; never remove a collided path.
        // Cleanup is best effort and runs on every return path, including I/O errors.
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub fn write_source(kernel: &KernelDescriptor, path: impl AsRef<Path>) -> Result<(), Error> {
    fs::write(path, kernel.slang_source).map_err(Error::Io)
}

/// Machine-readable record of whether the installed Slang compiler can emit a
/// target. Capability claims must follow this evidence, not assumption.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetProbe {
    /// The probed target.
    pub target: Target,
    /// Whether `slangc` compiled the known-good probe kernel to this target.
    pub supported: bool,
    /// Evidence: a success marker, or the compiler diagnostic on failure.
    pub detail: String,
}

/// A minimal, known-good compute kernel used only to probe target capability.
const PROBE_KERNEL: &str = concat!(
    "[shader(\"compute\")]\n",
    "[numthreads(1, 1, 1)]\n",
    "void gpu_probe_entry(uint3 id : SV_DispatchThreadID) {}\n",
);

/// Probe whether `slangc` can emit `target` by compiling a known-good kernel.
///
/// This is capability evidence, not a correctness test: it records whether the
/// target is supported and, if not, why, so downstream claims can cite it.
pub fn probe(target: Target) -> TargetProbe {
    match compile_source(PROBE_KERNEL, "gpu_probe_entry", target) {
        Ok(_) => TargetProbe {
            target,
            supported: true,
            detail: "compiled".to_string(),
        },
        Err(error) => TargetProbe {
            target,
            supported: false,
            detail: error.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mapped_kernel(source: &str, diagnostic: &str) -> Option<String> {
        map_slang_diagnostic(&SourceMap::from_slang(source), source, diagnostic)
            .map(|origin| origin.kernel)
    }

    #[test]
    fn never_claims_or_removes_an_existing_directory() {
        let owner = TemporaryDirectory::create().unwrap();
        let path = owner.0.clone();
        fs::write(path.join("sentinel"), "retain me").unwrap();
        assert!(
            TemporaryDirectory::try_create(path.clone())
                .unwrap()
                .is_none()
        );
        assert_eq!(
            fs::read_to_string(path.join("sentinel")).unwrap(),
            "retain me"
        );
        drop(owner);
        assert!(!path.exists());
    }

    #[test]
    fn compiler_failure_reports_diagnostics_and_cleans_owned_files() {
        let directory = TemporaryDirectory::create().unwrap();
        let path = directory.0.clone();
        let error = compile_in_directory(
            directory,
            "this is not Slang;",
            "missing",
            &SourceOrigin::UNKNOWN,
            Target::Wgsl,
        )
        .unwrap_err();
        assert!(matches!(error, Error::CompilationFailed { .. }), "{error}");
        assert!(error.to_string().contains("compiler exit:"));
        assert!(!path.exists());
    }

    #[test]
    fn early_io_failure_also_cleans_owned_files() {
        let directory = TemporaryDirectory::create().unwrap();
        let path = directory.0.clone();
        fs::create_dir(path.join("kernel.slang")).unwrap();
        assert!(matches!(
            compile_in_directory(
                directory,
                "",
                "missing",
                &SourceOrigin::UNKNOWN,
                Target::Wgsl,
            ),
            Err(Error::Io(_))
        ));
        assert!(!path.exists());
    }

    #[test]
    fn probe_records_wgsl_capability() {
        let probe = probe(Target::Wgsl);
        assert!(probe.supported, "WGSL should compile: {}", probe.detail);
    }

    #[test]
    fn probe_records_spirv_capability() {
        let probe = probe(Target::Spirv);
        assert!(probe.supported, "SPIR-V should compile: {}", probe.detail);
    }

    #[test]
    fn maps_slang_diagnostic_to_originating_rust_kernel() {
        let source = concat!(
            "// Generated by GUST\n",
            "// @rust kernel: vector_add::add\n",
            "[shader(\"compute\")]\n",
            "[numthreads(64, 1, 1)]\n",
            "void gpu_vector_add_add(uint3 id : SV_DispatchThreadID) {\n",
            "    int x = undefined_symbol;\n",
            "}\n",
        );
        let diagnostic = "kernel.slang(6): error: expected an expression\n";
        let origin = map_slang_diagnostic(&SourceMap::from_slang(source), source, diagnostic)
            .expect("should map");
        assert_eq!(origin.kernel, "vector_add::add");
        assert_eq!(origin.construct, "local");
    }

    /// Output shaped like the emitter's: a Result helper, the resource prologue
    /// and a lowered `match`. `scrutinee` and `arm` inject the failing text.
    fn match_kernel(signature: &str, scrutinee: &str, arm: &str) -> String {
        format!(
            concat!(
                "struct R {{ bool isOk; uint value; }};\n",
                "R pick() {{ R r; r.isOk = true; r.value = uint(1); return r; }}\n",
                "RWStructuredBuffer<uint> values;\n",
                "\n",
                "// @rust kernel: example::broken\n",
                "[shader(\"compute\")]\n",
                "[numthreads(1, 1, 1)]\n",
                "void broken({signature})\n",
                "{{\n",
                "    uint __gpu_len_values;\n",
                "    uint __gpu_stride_values;\n",
                "    values.GetDimensions(__gpu_len_values, __gpu_stride_values);\n",
                "    var total = uint(0);\n",
                "    // @gust construct: match #1\n",
                "    var __gust_match_1 = {scrutinee};\n",
                "    if (__gust_match_1.isOk)\n",
                "    {{\n",
                "        var h = __gust_match_1.value;\n",
                "        total = ({arm});\n",
                "    }}\n",
                "    else\n",
                "    {{\n",
                "        var e = __gust_match_1.value;\n",
                "        total = (total + e);\n",
                "    }}\n",
                "    values[id.x] = total;\n",
                "}}\n",
            ),
            signature = signature,
            scrutinee = scrutinee,
            arm = arm,
        )
    }

    const VALID_SIGNATURE: &str = "uint3 id : SV_DispatchThreadID";

    fn failure_diagnostics(source: &str, target: Target) -> String {
        let error = compile_source(source, "broken", target).unwrap_err();
        let Error::CompilationFailed { diagnostics, .. } = error else {
            panic!("expected actual compiler failure for {target:?}: {error}");
        };
        diagnostics
    }

    #[test]
    fn sidecar_map_file_is_written_before_slangc_runs_and_has_the_documented_schema() {
        let source = concat!(
            "RWStructuredBuffer<uint> values;\n",
            "\n",
            "// @rust kernel: example::ok\n",
            "[shader(\"compute\")]\n",
            "[numthreads(1, 1, 1)]\n",
            "void ok(uint3 id : SV_DispatchThreadID)\n",
            "{\n",
            "    uint __gpu_len_values;\n",
            "    uint __gpu_stride_values;\n",
            "    values.GetDimensions(__gpu_len_values, __gpu_stride_values);\n",
            "    var i = id.x;\n",
            "    if ((i < __gpu_len_values))\n",
            "    {\n",
            "        values[i] = uint(1);\n",
            "    }\n",
            "}\n",
        );
        let expected = concat!(
            "{\n  \"schema_version\": 2,\n  \"rust_file\": null,\n  \"segments\": [\n",
            "    {\"slang_start\":3,\"slang_end\":16,\"kernel\":\"example::ok\",\"construct\":\"kernel\",\"ordinal\":0},\n",
            "    {\"slang_start\":8,\"slang_end\":10,\"kernel\":\"example::ok\",\"construct\":\"prologue\",\"ordinal\":0},\n",
            "    {\"slang_start\":11,\"slang_end\":11,\"kernel\":\"example::ok\",\"construct\":\"local\",\"ordinal\":0},\n",
            "    {\"slang_start\":12,\"slang_end\":15,\"kernel\":\"example::ok\",\"construct\":\"conditional\",\"ordinal\":1},\n",
            "    {\"slang_start\":14,\"slang_end\":14,\"kernel\":\"example::ok\",\"construct\":\"assignment\",\"ordinal\":2}\n",
            "  ]\n}\n",
        );
        // A successful compile and a failing one both leave the sidecar behind
        // until the owner removes the directory.
        let directory = TemporaryDirectory::create().unwrap();
        compile_at(
            directory.path(),
            source,
            "ok",
            &SourceOrigin::UNKNOWN,
            Target::Wgsl,
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(directory.path().join("kernel.map.json")).unwrap(),
            expected
        );
        assert_eq!(
            fs::read_to_string(directory.path().join("kernel.slang")).unwrap(),
            source
        );

        let failing = TemporaryDirectory::create().unwrap();
        let broken = match_kernel(VALID_SIGNATURE, "undefined_symbol", "h");
        assert!(
            compile_at(
                failing.path(),
                &broken,
                "broken",
                &SourceOrigin::UNKNOWN,
                Target::Wgsl
            )
            .is_err()
        );
        let sidecar = fs::read_to_string(failing.path().join("kernel.map.json")).unwrap();
        assert_eq!(
            sidecar,
            SourceMap::from_slang(&broken).json(&SourceOrigin::UNKNOWN)
        );
        assert!(sidecar.contains("\"construct\":\"match\""), "{sidecar}");
    }

    #[test]
    fn sidecar_map_segments_follow_nested_blocks() {
        let source = concat!(
            "// @rust kernel: example::run\n",
            "void run(uint3 id : SV_DispatchThreadID)\n",
            "{\n",
            "    var value = id.x;\n",
            "    if (value > 0)\n",
            "    {\n",
            "        value = value - 1;\n",
            "    }\n",
            "}\n",
        );
        let mut kinds: Vec<_> = SourceMap::from_slang(source)
            .segments()
            .iter()
            .map(|segment| (segment.construct, segment.start_line, segment.end_line))
            .collect();
        kinds.sort_unstable();
        assert_eq!(
            kinds,
            vec![
                ("assignment", 7, 7),
                ("conditional", 5, 8),
                ("kernel", 1, 9),
                ("local", 4, 4),
            ]
        );
    }

    #[test]
    fn failure_inside_a_lowered_match_reports_the_match_on_each_target() {
        let source = match_kernel(VALID_SIGNATURE, "undefined_symbol", "h");
        for target in [Target::Wgsl, Target::Spirv] {
            let diagnostics = failure_diagnostics(&source, target);
            assert!(diagnostics.contains("undefined_symbol"), "{diagnostics}");
            assert!(
                diagnostics.contains("originating Rust kernel: example::broken"),
                "{target:?}: {diagnostics}"
            );
            // `total` is local #0, so the match is construct #1.
            assert!(
                diagnostics.contains("originating Rust construct: match #1"),
                "{target:?}: {diagnostics}"
            );
        }
    }

    #[test]
    fn match_label_uses_the_emitted_marker_number_not_the_map_ordinal() {
        // The map would count this match as construct #1; the emitter numbered it 7.
        let source = match_kernel(VALID_SIGNATURE, "undefined_symbol", "h")
            .replace("match_1", "match_7")
            .replace("match #1", "match #7");
        let diagnostics = failure_diagnostics(&source, Target::Wgsl);
        assert!(diagnostics.contains("undefined_symbol"), "{diagnostics}");
        assert!(
            diagnostics.contains("originating Rust construct: match #7"),
            "{diagnostics}"
        );
    }

    #[test]
    fn failure_in_a_user_statement_inside_a_match_arm_reports_that_statement() {
        let source = match_kernel(VALID_SIGNATURE, "pick()", "total + undefined_symbol");
        let diagnostics = failure_diagnostics(&source, Target::Wgsl);
        assert!(diagnostics.contains("undefined_symbol"), "{diagnostics}");
        assert!(
            diagnostics.contains("originating Rust construct: assignment #2"),
            "{diagnostics}"
        );
    }

    #[test]
    fn failure_on_an_unsegmented_kernel_line_falls_back_to_the_kernel() {
        let source = match_kernel("unknown_type id : SV_DispatchThreadID", "pick()", "h");
        let diagnostics = failure_diagnostics(&source, Target::Wgsl);
        assert!(
            diagnostics.contains("originating Rust kernel: example::broken"),
            "{diagnostics}"
        );
        assert!(
            diagnostics.contains("originating Rust construct: kernel #0"),
            "{diagnostics}"
        );
    }

    #[test]
    fn maps_slang_diagnostic_before_any_marker_returns_none() {
        let source = concat!(
            "// Generated by GUST\n",
            "[shader(\"compute\")]\n",
            "void gpu_entry(uint3 id : SV_DispatchThreadID) {\n",
            "    int x = undefined_symbol;\n",
            "}\n",
        );
        let diagnostic = "kernel.slang(3): error: expected an expression\n";
        assert!(mapped_kernel(source, diagnostic).is_none());
    }

    #[test]
    fn diagnostic_mapping_rejects_unrelated_files_and_invalid_lines() {
        let source = "// @rust kernel: example::first\nvoid first() {}\n";
        for diagnostic in [
            "otherkernel.slang(2): error: bad expression",
            "C:\\generated\\otherkernel.slang(2): error: bad expression",
            "kernel.slang(0): error: bad expression",
            "kernel.slang(3): error: bad expression",
            "kernel.slang(2,0): error: bad expression",
            "kernel.slang(2,1,4): error: bad expression",
        ] {
            assert_eq!(mapped_kernel(source, diagnostic), None, "{diagnostic}");
        }
    }

    #[test]
    fn diagnostic_mapping_accepts_absolute_paths_and_columns() {
        let source = "// @rust kernel: example::first\nvoid first() {}\n";
        for diagnostic in [
            "C:\\generated files\\kernel.slang(2): error 30015: bad expression",
            "/tmp/generated/kernel.slang(2,7): error: bad expression",
            "kernel.slang(2, 7): error: bad expression",
        ] {
            assert_eq!(
                mapped_kernel(source, diagnostic).as_deref(),
                Some("example::first"),
                "{diagnostic}"
            );
        }
    }

    #[test]
    fn diagnostic_mapping_prefers_error_over_warning_and_note() {
        let source = concat!(
            "// @rust kernel: example::first\nvoid first() {}\n",
            "// @rust kernel: example::second\nvoid second() {}\n",
        );
        let diagnostic = concat!(
            "kernel.slang(2): warning 1: first warning\n",
            "kernel.slang(2): note: first note\n",
            "kernel.slang(4,3): error 30015: actual failure\n",
        );
        assert_eq!(
            mapped_kernel(source, diagnostic).as_deref(),
            Some("example::second")
        );
    }

    #[test]
    fn diagnostic_mapping_reads_current_multiline_slang_format() {
        let source = "// @rust kernel: example::first\nvoid first() {}\n// @rust kernel: example::second\nvoid second() {}\n";
        let diagnostics = "warning[W1]: warning\n --> C:\\generated\\kernel.slang:2:1\nnote: extra context\nerror[E30015]: undefined identifier\n --> C:\\generated\\kernel.slang:4:7\n";
        assert_eq!(
            mapped_kernel(source, diagnostics).as_deref(),
            Some("example::second")
        );
        assert_eq!(
            mapped_kernel(
                source,
                &diagnostics.replace("kernel.slang", "otherkernel.slang")
            ),
            None
        );
    }

    #[test]
    fn compiler_failure_maps_originating_kernel_on_each_target() {
        let source = concat!(
            "// @rust kernel: example::broken\n",
            "[shader(\"compute\")]\n[numthreads(1, 1, 1)]\n",
            "void broken(uint3 id : SV_DispatchThreadID) {\n",
            "    int value = undefined_symbol;\n}\n",
        );
        for target in [Target::Wgsl, Target::Spirv] {
            let error = compile_source(source, "broken", target).unwrap_err();
            let Error::CompilationFailed { diagnostics, .. } = error else {
                panic!("expected actual compiler failure for {target:?}: {error}");
            };
            assert!(diagnostics.contains("undefined_symbol"), "{diagnostics}");
            assert!(
                diagnostics.contains("originating Rust kernel: example::broken"),
                "{target:?}: {diagnostics}"
            );
            assert!(
                diagnostics.contains("originating Rust construct: local #0"),
                "{target:?}: {diagnostics}"
            );
        }
    }

    /// Slang lines 1 to 8 of this source: marker, attributes, signature, `{`, three
    /// statements and `}`. The synthetic table below maps lines 6 and 7 only.
    const ANNOTATED_SOURCE: &str = concat!(
        "// @rust kernel: demo::k\n",               // 1
        "[shader(\"compute\")]\n",                  // 2
        "[numthreads(1, 1, 1)]\n",                  // 3
        "void k(uint3 id : SV_DispatchThreadID)\n", // 4
        "{\n",                                      // 5
        "    var a = uint(1);\n",                   // 6
        "    var b = (a + undefined_symbol);\n",    // 7
        "    var c = uint(3);\n",                   // 8
        "}\n",                                      // 9
    );
    const ANNOTATED_LINES: &[crate::LineSpan] = &[
        crate::LineSpan {
            slang_start: 6,
            slang_end: 6,
            rust_line: 21,
        },
        crate::LineSpan {
            slang_start: 7,
            slang_end: 7,
            rust_line: 22,
        },
    ];
    const ANNOTATED_ORIGIN: SourceOrigin = SourceOrigin {
        file: "crates/demo/src/lib.rs",
        lines: ANNOTATED_LINES,
    };

    fn annotate(origin: &SourceOrigin, diagnostics: &str) -> String {
        annotate_diagnostics(
            origin,
            ANNOTATED_SOURCE,
            &SourceMap::from_slang(ANNOTATED_SOURCE),
            diagnostics,
        )
    }

    #[test]
    fn annotation_appends_the_rust_source_line_of_the_innermost_record() {
        for diagnostic in [
            "kernel.slang(7): error: undefined identifier\n",
            "error[E30015]: undefined identifier\n --> C:\\tmp\\kernel.slang:7:17\n",
        ] {
            let annotated = annotate(&ANNOTATED_ORIGIN, diagnostic);
            assert!(annotated.starts_with(diagnostic), "{annotated}");
            assert!(
                annotated.ends_with(concat!(
                    "\noriginating Rust kernel: demo::k",
                    "\noriginating Rust construct: local #1",
                    "\noriginating Rust source: crates/demo/src/lib.rs:22",
                )),
                "{annotated}"
            );
        }
    }

    #[test]
    fn annotation_without_a_record_for_the_line_names_kernel_and_construct_only() {
        // Line 8 is inside the kernel but outside every record.
        let annotated = annotate(
            &ANNOTATED_ORIGIN,
            "kernel.slang(8): error: undefined identifier\n",
        );
        assert!(
            annotated.contains("originating Rust construct: local #2"),
            "{annotated}"
        );
        assert!(
            !annotated.contains("originating Rust source"),
            "{annotated}"
        );
        // A record outside this source's lines never matches a line it does not cover.
        let annotated = annotate(&ANNOTATED_ORIGIN, "kernel.slang(5): error: x\n");
        assert!(
            !annotated.contains("originating Rust source"),
            "{annotated}"
        );
    }

    #[test]
    fn unknown_origin_diagnostics_match_the_pre_line_table_text() {
        let diagnostic = "kernel.slang(7): error: undefined identifier\n";
        let annotated = annotate(&SourceOrigin::UNKNOWN, diagnostic);
        assert_eq!(
            annotated,
            format!(
                "{diagnostic}\noriginating Rust kernel: demo::k\noriginating Rust construct: local #1"
            )
        );
        // A table without a file is as good as none.
        let fileless = SourceOrigin {
            file: "",
            lines: ANNOTATED_LINES,
        };
        assert_eq!(annotate(&fileless, diagnostic), annotated);
    }

    #[test]
    fn a_helper_line_above_the_first_marker_gets_a_rust_line_but_no_kernel() {
        let source = concat!(
            "uint twice(uint x)\n",                 // 1
            "{\n",                                  // 2
            "    return (x + undefined_symbol);\n", // 3
            "}\n",                                  // 4
            "\n",                                   // 5
            "// @rust kernel: demo::k\n",           // 6
            "void k() {}\n",                        // 7
        );
        const LINES: &[crate::LineSpan] = &[
            crate::LineSpan {
                slang_start: 1,
                slang_end: 4,
                rust_line: 30,
            },
            crate::LineSpan {
                slang_start: 3,
                slang_end: 3,
                rust_line: 31,
            },
        ];
        let origin = SourceOrigin {
            file: "src/helpers.rs",
            lines: LINES,
        };
        let diagnostic = "kernel.slang(3): error: undefined identifier\n";
        let annotated =
            annotate_diagnostics(&origin, source, &SourceMap::from_slang(source), diagnostic);
        assert_eq!(
            annotated,
            format!("{diagnostic}\noriginating Rust source: src/helpers.rs:31")
        );
        // Without a table the same failure stays unattributed, as before.
        assert_eq!(
            annotate_diagnostics(
                &SourceOrigin::UNKNOWN,
                source,
                &SourceMap::from_slang(source),
                diagnostic
            ),
            diagnostic
        );
    }

    #[test]
    fn annotation_does_not_attribute_a_warning_or_a_foreign_file() {
        for diagnostic in [
            "warning[W1]: careful\n --> C:\\tmp\\kernel.slang:7:17\n",
            "error[E1]: bad\n --> crates/demo/src/lib.rs:7:17\n",
            "no location at all\n",
        ] {
            assert_eq!(annotate(&ANNOTATED_ORIGIN, diagnostic), diagnostic);
        }
    }

    #[test]
    fn sidecar_written_by_a_compile_carries_the_rust_file_and_lines() {
        let source = concat!(
            "RWStructuredBuffer<uint> values;\n",        // 1
            "\n",                                        // 2
            "// @rust kernel: example::ok\n",            // 3
            "[shader(\"compute\")]\n",                   // 4
            "[numthreads(1, 1, 1)]\n",                   // 5
            "void ok(uint3 id : SV_DispatchThreadID)\n", // 6
            "{\n",                                       // 7
            "    uint __gpu_len_values;\n",              // 8
            "    uint __gpu_stride_values;\n",           // 9
            "    values.GetDimensions(__gpu_len_values, __gpu_stride_values);\n", // 10
            "    var i = id.x;\n",                       // 11
            "    values[i] = uint(1);\n",                // 12
            "}\n",                                       // 13
        );
        const LINES: &[crate::LineSpan] = &[
            crate::LineSpan {
                slang_start: 11,
                slang_end: 11,
                rust_line: 7,
            },
            crate::LineSpan {
                slang_start: 12,
                slang_end: 12,
                rust_line: 8,
            },
        ];
        let origin = SourceOrigin {
            file: "src/kernels.rs",
            lines: LINES,
        };
        let directory = TemporaryDirectory::create().unwrap();
        compile_at(directory.path(), source, "ok", &origin, Target::Wgsl).unwrap();
        let sidecar = fs::read_to_string(directory.path().join("kernel.map.json")).unwrap();
        assert!(
            sidecar.starts_with(
                "{\n  \"schema_version\": 2,\n  \"rust_file\": \"src/kernels.rs\",\n  \"segments\": ["
            ),
            "{sidecar}"
        );
        assert!(
            sidecar.contains("\"construct\":\"local\",\"ordinal\":0,\"rust_line\":7}"),
            "{sidecar}"
        );
        assert!(
            sidecar.contains("\"construct\":\"assignment\",\"ordinal\":1,\"rust_line\":8}"),
            "{sidecar}"
        );
        // The prologue has no record, and neither does the kernel as a whole.
        assert!(
            sidecar.contains("\"construct\":\"prologue\",\"ordinal\":0}"),
            "{sidecar}"
        );
        // Compiling with a table does not change the text the compiler reads.
        assert_eq!(
            fs::read_to_string(directory.path().join("kernel.slang")).unwrap(),
            source
        );
    }

    #[test]
    fn malformed_compiler_artifacts_are_rejected() {
        assert!(matches!(
            decode_spirv(&[0, 1, 2]),
            Err(Error::InvalidSpirvLength(3))
        ));
        assert!(matches!(decode_spirv(&[]), Err(Error::InvalidSpirv(_))));
        assert!(matches!(
            decode_spirv(&[0; 20]),
            Err(Error::InvalidSpirv(_))
        ));
        let truncated = crate::spirv::words_as_le_bytes(&[
            0x0723_0203,
            0x0001_0000,
            0,
            2,
            0,
            (3 << 16) | 14,
            0,
        ]);
        assert!(matches!(
            decode_spirv(&truncated),
            Err(Error::InvalidSpirv(
                "SPIR-V instruction extends beyond the module"
            ))
        ));
    }
}

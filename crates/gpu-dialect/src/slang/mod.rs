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

use crate::KernelDescriptor;

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
    compile_source(kernel.slang_source, kernel.entry_point, target)
}

pub fn compile_spirv(kernel: &KernelDescriptor) -> Result<Vec<u32>, Error> {
    let bytes = compile(kernel, Target::Spirv)?;
    decode_spirv(&bytes)
}

pub(crate) fn decode_spirv(bytes: &[u8]) -> Result<Vec<u32>, Error> {
    if !bytes.len().is_multiple_of(size_of::<u32>()) {
        return Err(Error::InvalidSpirvLength(bytes.len()));
    }
    let words: Vec<_> = bytes
        .chunks_exact(size_of::<u32>())
        .map(|word| u32::from_le_bytes(word.try_into().expect("four-byte SPIR-V word")))
        .collect();
    crate::spirv::validate_structure(&words).map_err(Error::InvalidSpirv)?;
    Ok(words)
}

pub fn compile_wgsl(kernel: &KernelDescriptor) -> Result<String, Error> {
    let bytes = compile(kernel, Target::Wgsl)?;
    String::from_utf8(bytes)
        .map_err(|error| Error::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, error)))
}

pub fn compile_source(source: &str, entry: &str, target: Target) -> Result<Vec<u8>, Error> {
    compile_in_directory(TemporaryDirectory::create()?, source, entry, target)
}

fn compile_in_directory(
    directory: TemporaryDirectory,
    source: &str,
    entry: &str,
    target: Target,
) -> Result<Vec<u8>, Error> {
    let source_path = directory.0.join("kernel.slang");
    let output_path = directory.0.join(format!("kernel.{}", target.extension()));
    fs::write(&source_path, source)?;

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
        let diagnostics = map_slang_diagnostic(source, &diagnostics)
            .map(|kernel| format!("{diagnostics}\noriginating Rust kernel: {kernel}"))
            .unwrap_or(diagnostics);
        return Err(Error::CompilationFailed {
            target,
            diagnostics,
        });
    }
    let artifact = fs::read(&output_path)?;
    Ok(artifact)
}

/// Map a `slangc` diagnostic back to the originating Rust kernel name by
/// looking up the nearest `// @rust kernel:` marker at or before the reported
/// line. Kernel-level (not line-level) mapping, a stable-Rust constraint since
/// `proc_macro` spans don't expose line numbers.
fn map_slang_diagnostic(slang_source: &str, diagnostic: &str) -> Option<String> {
    let target_line = parse_slang_line(diagnostic)?;
    if target_line > slang_source.lines().count() {
        return None;
    }
    let mut kernel = None;
    for (index, line) in slang_source.lines().enumerate() {
        if index + 1 > target_line {
            break;
        }
        if let Some(name) = line.trim().strip_prefix("// @rust kernel: ") {
            kernel = (!name.trim().is_empty()).then(|| name.trim().to_owned());
        }
    }
    kernel
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
        if error_header {
            if let Some(location) = diagnostic.strip_prefix("--> ") {
                let (path_line, column) = location.rsplit_once(':')?;
                let (path, line) = path_line.rsplit_once(':')?;
                if path.rsplit(['/', '\\']).next()? != "kernel.slang" {
                    return None;
                }
                positive_coordinate(column)?;
                return positive_coordinate(line);
            }
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
            let path =
                std::env::temp_dir().join(format!("gpu-dialect-slang-{}-{id}", std::process::id()));
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
        let error = compile_in_directory(directory, "this is not Slang;", "missing", Target::Wgsl)
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
            compile_in_directory(directory, "", "missing", Target::Wgsl),
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
            "// Generated by gpu-dialect\n",
            "// @rust kernel: vector_add::add\n",
            "[shader(\"compute\")]\n",
            "[numthreads(64, 1, 1)]\n",
            "void gpu_vector_add_add(uint3 id : SV_DispatchThreadID) {\n",
            "    int x = undefined_symbol;\n",
            "}\n",
        );
        let diagnostic = "kernel.slang(6): error: expected an expression\n";
        let kernel = map_slang_diagnostic(source, diagnostic).expect("should map");
        assert_eq!(kernel, "vector_add::add");
    }

    #[test]
    fn maps_slang_diagnostic_before_any_marker_returns_none() {
        let source = concat!(
            "// Generated by gpu-dialect\n",
            "[shader(\"compute\")]\n",
            "void gpu_entry(uint3 id : SV_DispatchThreadID) {\n",
            "    int x = undefined_symbol;\n",
            "}\n",
        );
        let diagnostic = "kernel.slang(3): error: expected an expression\n";
        assert!(map_slang_diagnostic(source, diagnostic).is_none());
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
            assert_eq!(
                map_slang_diagnostic(source, diagnostic),
                None,
                "{diagnostic}"
            );
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
                map_slang_diagnostic(source, diagnostic).as_deref(),
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
            map_slang_diagnostic(source, diagnostic).as_deref(),
            Some("example::second")
        );
    }

    #[test]
    fn diagnostic_mapping_reads_current_multiline_slang_format() {
        let source = "// @rust kernel: example::first\nvoid first() {}\n// @rust kernel: example::second\nvoid second() {}\n";
        let diagnostics = "warning[W1]: warning\n --> C:\\generated\\kernel.slang:2:1\nnote: extra context\nerror[E30015]: undefined identifier\n --> C:\\generated\\kernel.slang:4:7\n";
        assert_eq!(
            map_slang_diagnostic(source, diagnostics).as_deref(),
            Some("example::second")
        );
        assert_eq!(
            map_slang_diagnostic(
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
        }
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

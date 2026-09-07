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

fn decode_spirv(bytes: &[u8]) -> Result<Vec<u32>, Error> {
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
        return Err(Error::CompilationFailed {
            target,
            diagnostics,
        });
    }
    let artifact = fs::read(&output_path)?;
    Ok(artifact)
}

struct TemporaryDirectory(PathBuf);

impl TemporaryDirectory {
    fn create() -> Result<Self, std::io::Error> {
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

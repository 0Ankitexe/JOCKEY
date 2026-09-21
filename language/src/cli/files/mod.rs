//! Explicit local file capabilities for V3 CLI adapters, never for compiler cores.
#[cfg(test)]
#[path = "../../../tests/support/artifacts.rs"]
mod artifacts;
mod paths;
#[cfg(unix)]
#[path = "unix.rs"]
mod platform;
#[cfg(windows)]
#[path = "windows.rs"]
mod platform;
#[cfg(not(any(unix, windows)))]
#[path = "unsupported.rs"]
mod platform;
mod publish;
mod read;
#[cfg(test)]
#[path = "unsupported.rs"]
mod unsupported_checks;
use std::path::Path;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactFailure {
    Input,
    Fixture,
    Resource,
    Output,
    UnsupportedIo,
}
impl ArtifactFailure {
    pub fn code(self) -> &'static str {
        match self {
            Self::Input => "V3_INPUT",
            Self::Fixture => "V3_FIXTURE",
            Self::Resource => "V3_RESOURCE",
            Self::Output => "V3_OUTPUT",
            Self::UnsupportedIo => "V3_UNSUPPORTED_IO",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactKind {
    Source,
    Ir,
    Fixture,
}
impl ArtifactKind {
    fn limit(self) -> usize {
        match self {
            Self::Source => jocky_ir::limits::SOURCE_BYTES,
            Self::Ir => jocky_ir::limits::IR_BYTES,
            Self::Fixture => jocky_ir::limits::FIXTURE_BYTES,
        }
    }
}

/// Fixture paths name a directory; other kinds name exactly one file. Implementors
/// must not resolve any strings found inside those files as additional paths.
pub trait ArtifactIo {
    fn read(&mut self, path: &Path, kind: ArtifactKind) -> Result<String, ArtifactFailure>;
    /// Caller supplies a fully verified/serialized buffer. Never overwrites.
    fn publish(&mut self, path: &Path, bytes: &[u8]) -> Result<(), ArtifactFailure>;
}
#[derive(Default)]
pub struct LocalArtifactIo;
impl ArtifactIo for LocalArtifactIo {
    fn read(&mut self, path: &Path, kind: ArtifactKind) -> Result<String, ArtifactFailure> {
        read::read(path, kind)
    }
    fn publish(&mut self, path: &Path, bytes: &[u8]) -> Result<(), ArtifactFailure> {
        publish::publish(path, bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsupported_platform_adapter_has_no_successful_io_fallback() {
        assert_eq!(
            unsupported_checks::validate(Path::new("inert")),
            Err(ArtifactFailure::UnsupportedIo)
        );
        let mut options = cap_std::fs::OpenOptions::new();
        unsupported_checks::nonblock(&mut options);
        let d = artifacts::OwnedDir::new("unsupported");
        let (dir, _) = paths::fixture(&d.0).unwrap();
        assert_eq!(
            unsupported_checks::metadata(&dir.dir_metadata().unwrap()),
            Err(ArtifactFailure::UnsupportedIo)
        );
    }
}

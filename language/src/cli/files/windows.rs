use super::{ArtifactFailure, paths};
use cap_std::fs::{Metadata, MetadataExt, OpenOptions};
use std::path::Path;
pub(super) fn nonblock(_: &mut OpenOptions) {}
pub(super) fn metadata(m: &Metadata) -> Result<(), ArtifactFailure> {
    const REPARSE_POINT: u32 = 0x400;
    if m.file_attributes() & REPARSE_POINT != 0 {
        Err(ArtifactFailure::Input)
    } else {
        Ok(())
    }
}
pub(super) fn validate(path: &Path) -> Result<(), ArtifactFailure> {
    if path.to_str().is_some_and(paths::valid_windows_text) {
        Ok(())
    } else {
        Err(ArtifactFailure::Input)
    }
}

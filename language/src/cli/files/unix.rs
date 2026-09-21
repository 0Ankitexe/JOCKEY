use super::ArtifactFailure;
use cap_fs_ext::OpenOptionsSyncExt;
use cap_std::fs::{Metadata, OpenOptions};
use std::path::{Component, Path};
pub(super) fn nonblock(o: &mut OpenOptions) {
    o.nonblock(true);
}
pub(super) fn metadata(m: &Metadata) -> Result<(), ArtifactFailure> {
    if m.is_symlink() {
        Err(ArtifactFailure::Input)
    } else {
        Ok(())
    }
}
pub(super) fn validate(path: &Path) -> Result<(), ArtifactFailure> {
    if path.components().any(|p| matches!(p, Component::ParentDir)) {
        Err(ArtifactFailure::Input)
    } else {
        Ok(())
    }
}

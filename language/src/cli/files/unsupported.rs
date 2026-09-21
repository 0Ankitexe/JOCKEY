use super::ArtifactFailure;
use cap_std::fs::{Metadata, OpenOptions};
use std::path::Path;
pub(super) fn nonblock(_: &mut OpenOptions) {}
pub(super) fn metadata(_: &Metadata) -> Result<(), ArtifactFailure> {
    Err(ArtifactFailure::UnsupportedIo)
}
pub(super) fn validate(_: &Path) -> Result<(), ArtifactFailure> {
    Err(ArtifactFailure::UnsupportedIo)
}

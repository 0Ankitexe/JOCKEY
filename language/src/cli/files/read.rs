#[cfg(test)]
use super::artifacts;
use super::{ArtifactFailure as Error, ArtifactKind, paths};
use std::{
    io::{self, Read},
    path::Path,
};

pub(super) fn read(path: &Path, kind: ArtifactKind) -> Result<String, Error> {
    let result = read_inner(path, kind);
    match result {
        Err(Error::Input) if kind == ArtifactKind::Fixture => Err(Error::Fixture),
        result => result,
    }
}
fn read_inner(path: &Path, kind: ArtifactKind) -> Result<String, Error> {
    let (dir, leaf) = if kind == ArtifactKind::Fixture {
        paths::fixture(path)?
    } else {
        paths::parent(path)?
    };
    paths::regular(&dir.symlink_metadata(&leaf).map_err(|_| Error::Input)?)?;
    let file = dir
        .open_with(&leaf, paths::options().read(true))
        .map_err(|_| Error::Input)?;
    paths::regular(&file.metadata().map_err(|_| Error::Input)?)?;
    bounded(file, kind.limit())
}
fn bounded(mut reader: impl Read, limit: usize) -> Result<String, Error> {
    let bound = limit.checked_add(1).ok_or(Error::Resource)?;
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 8192];
    while bytes.len() < bound {
        let capacity = (bound - bytes.len()).min(chunk.len());
        let n = match reader.read(&mut chunk[..capacity]) {
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            result => result.map_err(|_| Error::Input)?,
        };
        if n == 0 {
            break;
        }
        bytes.try_reserve_exact(n).map_err(|_| Error::Resource)?;
        bytes.extend_from_slice(&chunk[..n]);
    }
    if bytes.len() > limit {
        return Err(Error::Resource);
    }
    String::from_utf8(bytes).map_err(|_| Error::Input)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[test]
    fn retained_windows_parent_does_not_follow_replacement_junction() {
        let d = artifacts::OwnedDir::new("retained-windows");
        std::fs::create_dir(d.path("parent")).unwrap();
        std::fs::create_dir(d.path("other")).unwrap();
        std::fs::write(d.path("parent/file"), b"original").unwrap();
        std::fs::write(d.path("other/file"), b"replacement").unwrap();
        let (dir, leaf) = paths::parent(&d.path("parent/file")).unwrap();
        std::fs::rename(d.path("parent"), d.path("retained")).unwrap();
        assert!(
            std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(d.path("parent"))
                .arg(d.path("other"))
                .status()
                .unwrap()
                .success()
        );
        let file = dir.open_with(&leaf, paths::options().read(true)).unwrap();
        paths::regular(&file.metadata().unwrap()).unwrap();
        assert_eq!(bounded(file, 100).unwrap(), "original");
        assert!(read(&d.path("parent/file"), ArtifactKind::Source).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn retained_parent_and_leaf_nofollow_survive_name_substitution() {
        use std::os::unix::fs::symlink;
        let d = artifacts::OwnedDir::new("retained");
        std::fs::create_dir(d.path("parent")).unwrap();
        std::fs::create_dir(d.path("other")).unwrap();
        std::fs::write(d.path("parent/file"), b"original").unwrap();
        std::fs::write(d.path("other/file"), b"replacement").unwrap();
        let (dir, leaf) = paths::parent(&d.path("parent/file")).unwrap();
        std::fs::rename(d.path("parent"), d.path("retained")).unwrap();
        symlink(d.path("other"), d.path("parent")).unwrap();
        let f = dir.open_with(&leaf, paths::options().read(true)).unwrap();
        paths::regular(&f.metadata().unwrap()).unwrap();
        assert_eq!(bounded(f, 100).unwrap(), "original");
        paths::regular(&dir.symlink_metadata(&leaf).unwrap()).unwrap();
        dir.remove_file(&leaf).unwrap();
        symlink(d.path("other/file"), d.path("retained/file")).unwrap();
        assert!(dir.open_with(&leaf, paths::options().read(true)).is_err());
        assert!(
            std::process::Command::new("mkfifo")
                .arg(d.path("retained/fifo"))
                .status()
                .unwrap()
                .success()
        );
        let fifo = dir.open_with("fifo", paths::options().read(true)).unwrap();
        assert!(paths::regular(&fifo.metadata().unwrap()).is_err());
    }
    #[test]
    fn bounded_read_never_requests_more_than_limit_plus_one() {
        let mut r = io::Cursor::new(vec![b'a'; 100]);
        assert_eq!(bounded(&mut r, 4), Err(Error::Resource));
        assert_eq!(r.position(), 5);
        assert_eq!(bounded(io::Cursor::new(b"abcd"), 4).unwrap(), "abcd");
        assert_eq!(
            bounded(io::Cursor::new(b""), usize::MAX),
            Err(Error::Resource)
        );
    }
}

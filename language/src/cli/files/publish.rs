#[cfg(test)]
use super::artifacts;
use super::{ArtifactFailure as Error, paths};
use cap_std::fs::{Dir, File};
use std::{
    ffi::OsStr,
    io::{self, Write},
    path::Path,
};

trait Operations {
    fn write(&mut self, file: &mut File, bytes: &[u8]) -> io::Result<usize> {
        file.write(bytes)
    }
    fn flush(&mut self, file: &mut File) -> io::Result<()> {
        file.flush()
    }
    fn sync(&mut self, file: &File) -> io::Result<()> {
        file.sync_all()
    }
    fn link(&mut self, dir: &Dir, stage: &str, leaf: &OsStr) -> io::Result<()> {
        dir.hard_link(stage, dir, leaf)
    }
    fn cleanup(&mut self, dir: &Dir, stage: &str) -> io::Result<()> {
        dir.remove_file(stage)
    }
}
struct Real;
impl Operations for Real {}

pub(super) fn publish(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    publish_with(path, bytes, &mut Real)
}
fn publish_with(path: &Path, bytes: &[u8], ops: &mut impl Operations) -> Result<(), Error> {
    if bytes.len() > jocky_ir::limits::IR_BYTES {
        return Err(Error::Resource);
    }
    let (dir, leaf) = paths::parent(path).map_err(|e| {
        if e == Error::UnsupportedIo {
            e
        } else {
            Error::Output
        }
    })?;
    match dir.symlink_metadata(&leaf) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        _ => return Err(Error::Output),
    }
    let mut stage = None;
    for candidate in 0..64 {
        let name = format!(".jocky-ir-stage-{candidate:04}.tmp");
        if leaf == OsStr::new(&name) {
            continue;
        }
        match dir.open_with(&name, paths::options().write(true).create_new(true)) {
            Ok(file) => {
                stage = Some((name, file));
                break;
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(Error::Output),
        }
    }
    let (stage, mut file) = stage.ok_or(Error::Output)?;
    let result = (|| {
        paths::regular(&file.metadata().map_err(|_| Error::Output)?).map_err(|_| Error::Output)?;
        let mut remaining = bytes;
        while !remaining.is_empty() {
            let n = match ops.write(&mut file, remaining) {
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                result => result.map_err(|_| Error::Output)?,
            };
            if n == 0 || n > remaining.len() {
                return Err(Error::Output);
            }
            remaining = &remaining[n..];
        }
        ops.flush(&mut file).map_err(|_| Error::Output)?;
        ops.sync(&file).map_err(|_| Error::Output)?;
        ops.link(&dir, &stage, &leaf).map_err(|e| {
            if e.kind() == io::ErrorKind::Unsupported {
                Error::UnsupportedIo
            } else {
                Error::Output
            }
        })
    })();
    drop(file);
    // Only our staging entry is removed. A final artifact is never rollback data.
    if ops.cleanup(&dir, &stage).is_err() {
        return Err(Error::Output);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Clone, Copy, PartialEq)]
    enum Fault {
        Short,
        Interrupted,
        Overreported,
        Write,
        Zero,
        Partial,
        Flush,
        Sync,
        Link,
        Unsupported,
        Cleanup,
        Race,
    }
    struct Fail(Fault, usize);
    fn bad<T>() -> io::Result<T> {
        Err(io::Error::other("private injected failure"))
    }
    impl Operations for Fail {
        fn write(&mut self, f: &mut File, b: &[u8]) -> io::Result<usize> {
            self.1 += 1;
            if self.0 == Fault::Interrupted && self.1 == 1 {
                return Err(io::ErrorKind::Interrupted.into());
            }
            if self.0 == Fault::Overreported {
                return Ok(b.len() + 1);
            }
            if self.0 == Fault::Zero {
                return Ok(0);
            }
            if self.0 == Fault::Write || (self.0 == Fault::Partial && self.1 > 1) {
                bad()
            } else if matches!(self.0, Fault::Short | Fault::Partial) {
                f.write(&b[..b.len().min(2)])
            } else {
                f.write(b)
            }
        }
        fn flush(&mut self, f: &mut File) -> io::Result<()> {
            if self.0 == Fault::Flush {
                bad()
            } else {
                f.flush()
            }
        }
        fn sync(&mut self, f: &File) -> io::Result<()> {
            if self.0 == Fault::Sync {
                bad()
            } else {
                f.sync_all()
            }
        }
        fn link(&mut self, d: &Dir, s: &str, l: &OsStr) -> io::Result<()> {
            match self.0 {
                Fault::Link => bad(),
                Fault::Unsupported => Err(io::ErrorKind::Unsupported.into()),
                Fault::Race => {
                    d.write(l, b"raced-existing")?;
                    d.hard_link(s, d, l)
                }
                _ => d.hard_link(s, d, l),
            }
        }
        fn cleanup(&mut self, d: &Dir, s: &str) -> io::Result<()> {
            if self.0 == Fault::Cleanup {
                bad()
            } else {
                d.remove_file(s)
            }
        }
    }
    #[test]
    fn publication_failure_seams_never_claim_success_or_delete_final() {
        for fault in [
            Fault::Short,
            Fault::Interrupted,
            Fault::Overreported,
            Fault::Write,
            Fault::Zero,
            Fault::Partial,
            Fault::Flush,
            Fault::Sync,
            Fault::Link,
            Fault::Unsupported,
            Fault::Cleanup,
            Fault::Race,
        ] {
            let d = artifacts::OwnedDir::new("fault");
            let output = d.path("out");
            let result = publish_with(&output, b"complete-buffer", &mut Fail(fault, 0));
            if matches!(fault, Fault::Short | Fault::Interrupted) {
                result.unwrap();
            } else {
                assert_eq!(
                    result,
                    Err(if fault == Fault::Unsupported {
                        Error::UnsupportedIo
                    } else {
                        Error::Output
                    })
                );
            }
            if matches!(fault, Fault::Short | Fault::Interrupted | Fault::Cleanup) {
                assert_eq!(std::fs::read(&output).unwrap(), b"complete-buffer");
            } else if fault == Fault::Race {
                assert_eq!(std::fs::read(&output).unwrap(), b"raced-existing");
            } else {
                assert!(!output.exists());
            }
            assert_eq!(
                d.path(".jocky-ir-stage-0000.tmp").exists(),
                fault == Fault::Cleanup
            );
            if fault == Fault::Cleanup {
                assert_eq!(publish(&output, b"retry"), Err(Error::Output));
            }
        }
    }
}

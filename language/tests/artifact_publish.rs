#[path = "support/artifacts.rs"]
mod artifacts;
use artifacts::OwnedDir;
use jocky_language::cli::files::{ArtifactFailure as Error, ArtifactIo, LocalArtifactIo};

#[test]
fn publication_size_guard_precedes_staging_and_existing_final_is_unchanged() {
    let d = OwnedDir::new("publish-size");
    let mut io = LocalArtifactIo;
    let existing = d.write("existing", "unchanged");
    let oversized = vec![b' '; jocky_ir::limits::IR_BYTES + 1];
    assert_eq!(io.publish(&d.path("out"), &oversized), Err(Error::Resource));
    assert_eq!(io.publish(&existing, &oversized), Err(Error::Resource));
    assert_eq!(std::fs::read(&existing).unwrap(), b"unchanged");
    assert_eq!(std::fs::read_dir(&d.0).unwrap().count(), 1);
}

#[test]
fn exclusive_publication_preserves_existing_files_and_aliases() {
    let dir = OwnedDir::new("publish");
    let mut io = LocalArtifactIo;
    let source = dir.write("source", "original");
    let alias = dir.path("alias");
    std::fs::hard_link(&source, &alias).unwrap();
    for path in [&source, &alias] {
        assert_eq!(io.publish(path, b"new"), Err(Error::Output));
        assert_eq!(std::fs::read(path).unwrap(), b"original");
    }
    let out = dir.path("new.json");
    io.publish(&out, b"complete\n").unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), b"complete\n");
    assert_eq!(io.publish(&out, b"replacement"), Err(Error::Output));
    assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 3);
    assert!(io.publish(&dir.path("missing/out"), b"x").is_err());
}

#[test]
fn staging_names_are_bounded_and_never_reused_or_truncated() {
    let dir = OwnedDir::new("stages");
    let mut io = LocalArtifactIo;
    for n in 0..64 {
        dir.write(&format!(".jocky-ir-stage-{n:04}.tmp"), "owned elsewhere");
    }
    assert_eq!(io.publish(&dir.path("out"), b"new"), Err(Error::Output));
    assert!(!dir.path("out").exists());
    for n in 0..64 {
        assert_eq!(
            std::fs::read(dir.path(&format!(".jocky-ir-stage-{n:04}.tmp"))).unwrap(),
            b"owned elsewhere"
        );
    }
}

#[cfg(unix)]
#[test]
fn publication_refuses_symlink_parents_and_leaves() {
    use std::os::unix::fs::symlink;
    let dir = OwnedDir::new("publish-links");
    let mut io = LocalArtifactIo;
    let source = dir.write("source", "keep");
    symlink(&source, dir.path("leaf")).unwrap();
    assert!(io.publish(&dir.path("leaf"), b"new").is_err());
    std::fs::create_dir(dir.path("sub")).unwrap();
    symlink(dir.path("sub"), dir.path("parent")).unwrap();
    assert!(io.publish(&dir.path("parent/out"), b"new").is_err());
    assert!(!dir.path("sub/out").exists());
    assert_eq!(std::fs::read(source).unwrap(), b"keep");
}

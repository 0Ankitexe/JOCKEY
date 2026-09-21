#[path = "support/artifacts.rs"]
mod artifacts;
use artifacts::OwnedDir;
use jocky_language::cli::files::{
    ArtifactFailure as Error, ArtifactIo, ArtifactKind as Kind, LocalArtifactIo,
};

#[test]
fn explicit_reads_utf8_limits_and_fixed_fixture_leaf() {
    let dir = OwnedDir::new("reads");
    let mut io = LocalArtifactIo;
    let p = dir.write("source.jky", "hello λ\r\n");
    assert_eq!(io.read(&p, Kind::Source).unwrap(), "hello λ\r\n");
    assert_eq!(
        io.read(&dir.write("bad", [255]), Kind::Source),
        Err(Error::Input)
    );
    assert_eq!(
        io.read(&dir.path("missing"), Kind::Source),
        Err(Error::Input)
    );
    dir.write("fixture.json", "{}");
    dir.write("expected-result.json", "ignored");
    assert_eq!(io.read(&dir.0, Kind::Fixture).unwrap(), "{}");
    for (kind, limit) in [
        (Kind::Source, 4 * 1024 * 1024),
        (Kind::Ir, 16 * 1024 * 1024),
        (Kind::Fixture, 8 * 1024 * 1024),
    ] {
        let p = dir.write("fixture.json", vec![b' '; limit]);
        let input = if kind == Kind::Fixture { &dir.0 } else { &p };
        assert_eq!(io.read(input, kind).unwrap().len(), limit);
        dir.write("fixture.json", vec![b' '; limit + 1]);
        assert_eq!(io.read(input, kind), Err(Error::Resource));
    }
    assert!(io.read(&dir.path("../outside"), Kind::Source).is_err());
    assert!(io.read(&dir.0, Kind::Source).is_err());
}

#[cfg(unix)]
#[test]
fn symlink_components_leaves_and_fifo_are_rejected_without_waiting() {
    use std::os::unix::fs::symlink;
    let dir = OwnedDir::new("links");
    let mut io = LocalArtifactIo;
    let p = dir.write("real", "data");
    symlink(&p, dir.path("leaf")).unwrap();
    assert_eq!(io.read(&dir.path("leaf"), Kind::Ir), Err(Error::Input));
    std::fs::create_dir(dir.path("sub")).unwrap();
    symlink(dir.path("sub"), dir.path("parent")).unwrap();
    std::fs::write(dir.path("sub/a"), b"x").unwrap();
    assert!(io.read(&dir.path("parent/a"), Kind::Source).is_err());
    assert!(
        std::process::Command::new("mkfifo")
            .arg(dir.path("fifo"))
            .status()
            .unwrap()
            .success()
    );
    assert!(io.read(&dir.path("fifo"), Kind::Source).is_err());
}

#[cfg(windows)]
#[test]
fn nonlocal_windows_path_forms_are_rejected_before_opening() {
    let mut io = LocalArtifactIo;
    for path in [
        r"\\server\share\x",
        r"\\?\C:\x",
        r"\\.\NUL",
        r"C:relative",
        r"C:\x:stream",
        r"NUL",
        r"CON.txt",
    ] {
        assert_eq!(
            io.read(std::path::Path::new(path), Kind::Source),
            Err(Error::Input)
        );
    }
}

#[cfg(windows)]
#[test]
fn windows_junction_is_rejected_for_read_and_publication() {
    let d = OwnedDir::new("junction");
    let target = d.path("real");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("file"), b"owned").unwrap();
    let junction = d.path("junction");
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&junction)
        .arg(&target)
        .status()
        .unwrap();
    assert!(status.success(), "local junction fixture setup failed");
    let mut io = LocalArtifactIo;
    assert!(io.read(&junction.join("file"), Kind::Source).is_err());
    assert!(io.read(&junction, Kind::Fixture).is_err());
    assert!(io.publish(&junction.join("out"), b"new").is_err());
    assert!(!target.join("out").exists());
}

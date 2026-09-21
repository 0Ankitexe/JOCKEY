#[path = "support/artifacts.rs"]
mod artifacts;
use artifacts::OwnedDir;
use jocky_language::cli::files::{ArtifactIo, ArtifactKind as Kind, LocalArtifactIo};

#[test]
fn fixture_leaf_directory_traversal_and_reference_files_are_never_followed() {
    let d = OwnedDir::new("confinement");
    std::fs::create_dir(d.path("fixture.json")).unwrap();
    d.write("outside.json", "do not load");
    let mut io = LocalArtifactIo;
    assert!(io.read(&d.0, Kind::Fixture).is_err());
    assert!(
        io.read(&d.path("fixture.json/../outside.json"), Kind::Ir)
            .is_err()
    );
    assert!(
        io.publish(&d.path("fixture.json/../new.json"), b"new")
            .is_err()
    );
    assert_eq!(
        std::fs::read(d.path("outside.json")).unwrap(),
        b"do not load"
    );
    assert!(!d.path("new.json").exists());
}

#[cfg(unix)]
#[test]
fn unix_socket_and_replaced_fixture_parent_or_leaf_are_rejected() {
    use std::os::unix::{fs::symlink, net::UnixListener};
    let d = OwnedDir::new("socket");
    let socket = d.path("socket");
    // sockaddr_un bounds the supplied path string, not the resolved file path.
    // Rebase the test-owned name without changing process-global cwd or using
    // an external temp directory, so deeply nested clean copies still work.
    let current = std::env::current_dir().unwrap();
    let mut ancestor = current.as_path();
    let mut relative = std::path::PathBuf::new();
    while !socket.starts_with(ancestor) {
        ancestor = ancestor.parent().unwrap();
        relative.push("..");
    }
    relative.push(socket.strip_prefix(ancestor).unwrap());
    let listener = UnixListener::bind(&relative).unwrap();
    let mut io = LocalArtifactIo;
    assert!(io.read(&socket, Kind::Source).is_err());
    assert!(io.publish(&socket, b"do not overwrite").is_err());
    std::fs::create_dir(d.path("original")).unwrap();
    std::fs::create_dir(d.path("other")).unwrap();
    std::fs::write(d.path("original/fixture.json"), "original").unwrap();
    std::fs::write(d.path("other/fixture.json"), "other").unwrap();
    assert_eq!(
        io.read(&d.path("original"), Kind::Fixture).unwrap(),
        "original"
    );
    std::fs::rename(d.path("original"), d.path("retained")).unwrap();
    symlink(d.path("other"), d.path("original")).unwrap();
    assert!(io.read(&d.path("original"), Kind::Fixture).is_err());
    std::fs::rename(d.path("retained/fixture.json"), d.path("saved")).unwrap();
    symlink(
        d.path("other/fixture.json"),
        d.path("retained/fixture.json"),
    )
    .unwrap();
    assert!(io.read(&d.path("retained"), Kind::Fixture).is_err());
    assert_eq!(std::fs::read(d.path("saved")).unwrap(), b"original");
    drop(listener);
}

#[cfg(windows)]
#[test]
fn replaced_fixture_directory_junction_and_device_stream_names_fail_closed() {
    let d = OwnedDir::new("reparse");
    std::fs::create_dir(d.path("original")).unwrap();
    std::fs::create_dir(d.path("other")).unwrap();
    std::fs::write(d.path("original/fixture.json"), "original").unwrap();
    std::fs::write(d.path("other/fixture.json"), "other").unwrap();
    let mut io = LocalArtifactIo;
    assert_eq!(
        io.read(&d.path("original"), Kind::Fixture).unwrap(),
        "original"
    );
    std::fs::rename(d.path("original"), d.path("retained")).unwrap();
    assert!(
        std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(d.path("original"))
            .arg(d.path("other"))
            .status()
            .unwrap()
            .success()
    );
    assert!(io.read(&d.path("original"), Kind::Fixture).is_err());
    assert!(io.publish(&d.path("original/new.json"), b"new").is_err());
    for name in [
        "NUL",
        "CON.txt",
        "COM1",
        "data:stream",
        "..\\other\\fixture.json",
    ] {
        assert!(io.read(&d.path(name), Kind::Ir).is_err());
    }
    assert!(!d.path("other/new.json").exists());
}

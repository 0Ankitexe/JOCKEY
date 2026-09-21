#![allow(dead_code)]
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};
pub struct OwnedDir(pub PathBuf);
impl OwnedDir {
    pub fn new(label: &str) -> Self {
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        let base = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("target/v3-tests");
        std::fs::create_dir_all(&base).unwrap();
        for _ in 0..64 {
            let p = base.join(format!(
                "{label}-{}-{}",
                std::process::id(),
                SEQ.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&p) {
                Ok(()) => return Self(p),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("owned test directory: {e}"),
            }
        }
        panic!("test directory collisions")
    }
    pub fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
    pub fn write(&self, name: &str, bytes: impl AsRef<[u8]>) -> PathBuf {
        let p = self.path(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }
}
impl Drop for OwnedDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

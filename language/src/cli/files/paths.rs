use super::{ArtifactFailure as Error, platform};
use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt};
use cap_std::{
    ambient_authority,
    fs::{Dir, Metadata, OpenOptions},
};
use std::{
    ffi::OsString,
    path::{Component, Path, PathBuf},
};

pub(super) fn options() -> OpenOptions {
    let mut o = OpenOptions::new();
    o.follow(FollowSymlinks::No);
    platform::nonblock(&mut o);
    o
}
pub(super) fn regular(m: &Metadata) -> Result<(), Error> {
    platform::metadata(m)?;
    if m.is_file() && !m.is_symlink() {
        Ok(())
    } else {
        Err(Error::Input)
    }
}
fn directory(m: &Metadata) -> Result<(), Error> {
    platform::metadata(m)?;
    if m.is_dir() && !m.is_symlink() {
        Ok(())
    } else {
        Err(Error::Input)
    }
}
fn walk(path: &Path) -> Result<Dir, Error> {
    platform::validate(path)?;
    let mut root = PathBuf::new();
    for c in path.components() {
        match c {
            Component::Prefix(p) => root.push(p.as_os_str()),
            Component::RootDir => root.push(c.as_os_str()),
            Component::ParentDir => return Err(Error::Input),
            _ => {}
        }
    }
    if root.as_os_str().is_empty() {
        root.push(".");
    }
    // Ambient authority exists only here, to anchor the explicit local path.
    let mut dir = Dir::open_ambient_dir(root, ambient_authority()).map_err(|_| Error::Input)?;
    directory(&dir.dir_metadata().map_err(|_| Error::Input)?)?;
    for c in path.components() {
        if let Component::Normal(name) = c {
            directory(&dir.symlink_metadata(name).map_err(|_| Error::Input)?)?;
            dir = dir.open_dir_nofollow(name).map_err(|_| Error::Input)?;
            directory(&dir.dir_metadata().map_err(|_| Error::Input)?)?;
        }
    }
    Ok(dir)
}
pub(super) fn parent(path: &Path) -> Result<(Dir, OsString), Error> {
    platform::validate(path)?;
    let Some(Component::Normal(leaf)) = path.components().next_back() else {
        return Err(Error::Input);
    };
    let parent = walk(path.parent().unwrap_or(Path::new(".")))?;
    Ok((parent, leaf.to_owned()))
}
pub(super) fn fixture(path: &Path) -> Result<(Dir, OsString), Error> {
    Ok((walk(path)?, OsString::from("fixture.json")))
}

#[cfg(any(windows, test))]
pub(super) fn valid_windows_text(text: &str) -> bool {
    let text = text.replace('/', "\\");
    if text.starts_with("\\\\") || text.starts_with('\\') {
        return false;
    }
    let body = if text.as_bytes().get(1) == Some(&b':') {
        if !text.as_bytes()[0].is_ascii_alphabetic() || text.as_bytes().get(2) != Some(&b'\\') {
            return false;
        }
        &text[3..]
    } else {
        &text
    };
    if body.contains(':') {
        return false;
    }
    body.split('\\').all(|p| {
        if p == ".." {
            return false;
        }
        if p.is_empty() || p == "." {
            return true;
        }
        let base = p.split('.').next().unwrap_or("").to_ascii_uppercase();
        !p.ends_with([' ', '.'])
            && !matches!(
                base.as_str(),
                "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
            )
            && !(base.len() == 4
                && (base.starts_with("COM") || base.starts_with("LPT"))
                && matches!(base.as_bytes()[3], b'1'..=b'9'))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_path_rules_are_testable_without_host_access() {
        for bad in [
            r"\\host\share",
            r"\\?\C:\a",
            r"\\.\NUL",
            r"C:a",
            r"C:\a:b",
            r"NUL",
            r"CON.txt",
            r"COM9",
            r"a\..\b",
            r"a.\b",
            r"\a",
        ] {
            assert!(!valid_windows_text(bad), "{bad}");
        }
        for good in [r"C:\a\b", r"a\b", r".\a", r"a/b", r"C:/a", "."] {
            assert!(valid_windows_text(good), "{good}");
        }
    }
}

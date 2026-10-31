//! Capability-based access to files and directories named by paths the engine is given.
//!
//! Policy directories, debug output locations, prompt files and similar paths
//! come from the command line, the environment or the platform's configuration
//! directories, so none of them is known to lie beneath a directory this
//! program already holds. Each helper opens the path's parent directory as a
//! [`Dir`] and acts on the file name beneath it, so file access goes through a
//! directory capability rather than `std::fs`. This is the one module that
//! calls `ambient_authority`: it acquires authority over whichever parent it is
//! given and does not confine a path to a preselected root.
//!
//! `cap-std` refuses to follow a symlink whose target escapes the opened
//! directory, including any absolute target. `std::fs` follows such links, and
//! some callers depend on that: script inspection must read the script a
//! symlink points at, or a symlink would hide the script from policy. So the
//! helpers resolve a symlink in the final path component themselves, with
//! `read_link_contents`, and then open the target's parent directory in turn.
//! Symlinks in the directory part of the path are resolved by the operating
//! system, as before.

use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};

use cap_std::ambient_authority;
use cap_std::fs::Dir;

/// Split `path` into the directory that holds it and the file name.
fn parent_and_name(path: &Path) -> io::Result<(&Path, &OsStr)> {
    let name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} does not name a file", path.display()),
        )
    })?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    Ok((parent, name))
}

/// The most symlinks followed in the final path component, matching the usual
/// operating system limit: a chain of this many links resolves, one more fails.
const MAX_SYMLINK_HOPS: usize = 40;

/// Follow symlinks in the final component of `path`, as `std::fs` does.
///
/// A path that does not exist, or is not a symlink, is returned unchanged so
/// that the caller's own open reports the error or creates the file. A dangling
/// symlink resolves to its target, which `std::fs::write` would create.
fn resolve_final_symlinks(path: &Path) -> io::Result<PathBuf> {
    let mut current = path.to_path_buf();
    let mut hops = 0;
    loop {
        let (parent, name) = parent_and_name(&current)?;
        let dir = Dir::open_ambient_dir(parent, ambient_authority())?;
        match dir.symlink_metadata(name) {
            Ok(metadata) if metadata.is_symlink() => {
                if hops == MAX_SYMLINK_HOPS {
                    return Err(io::Error::other(format!(
                        "too many levels of symbolic links: {}",
                        path.display()
                    )));
                }
                hops += 1;
                let target = dir.read_link_contents(name)?;
                current = parent.join(target);
            }
            _ => return Ok(current),
        }
    }
}

/// Open the directory that holds `path`, after following final symlinks, as a
/// capability. Returns the directory and the file name beneath it.
fn open_parent(path: &Path) -> io::Result<(Dir, std::ffi::OsString)> {
    let resolved = resolve_final_symlinks(path)?;
    let (parent, name) = parent_and_name(&resolved)?;
    Ok((
        Dir::open_ambient_dir(parent, ambient_authority())?,
        name.to_os_string(),
    ))
}

/// Read the whole file at `path` into a string.
///
/// # Errors
///
/// Returns the I/O error when the parent directory or the file cannot be
/// opened, or when the contents are not valid UTF-8.
pub fn read_to_string(path: impl AsRef<Path>) -> io::Result<String> {
    let (dir, name) = open_parent(path.as_ref())?;
    dir.read_to_string(&name)
}

/// Write `contents` to the file at `path`, creating or truncating it.
///
/// # Errors
///
/// Returns the I/O error when the parent directory cannot be opened or the file
/// cannot be written.
pub fn write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> io::Result<()> {
    let (dir, name) = open_parent(path.as_ref())?;
    dir.write(&name, contents)
}

/// Create the directory at `path` and any missing ancestors.
///
/// # Errors
///
/// Returns the I/O error when a directory cannot be created.
pub fn create_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
    Dir::create_ambient_dir_all(path.as_ref(), ambient_authority())
}

/// List the entries of the directory at `path` as paths beneath it.
///
/// The order is the operating system's, as with `std::fs::read_dir`.
///
/// # Errors
///
/// Returns the I/O error when the directory cannot be opened or read.
pub fn read_dir(path: impl AsRef<Path>) -> io::Result<Vec<PathBuf>> {
    let path = path.as_ref();
    Dir::open_ambient_dir(path, ambient_authority())?
        .entries()?
        .map(|entry| entry.map(|entry| path.join(entry.file_name())))
        .collect()
}

/// Read the Unix mode bits of the file or directory at `path`, following
/// symlinks as `std::fs::metadata` does.
///
/// # Errors
///
/// Returns the I/O error when the parent directory cannot be opened or the path
/// cannot be inspected.
#[cfg(unix)]
pub fn unix_mode(path: impl AsRef<Path>) -> io::Result<u32> {
    use cap_std::fs::PermissionsExt;

    let (dir, name) = open_parent(path.as_ref())?;
    Ok(dir.metadata(&name)?.permissions().mode())
}

#[cfg(test)]
mod tests;

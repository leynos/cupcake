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
/// operating system limit.
const MAX_SYMLINK_HOPS: usize = 40;

/// Follow symlinks in the final component of `path`, as `std::fs` does.
///
/// A path that does not exist, or is not a symlink, is returned unchanged so
/// that the caller's own open reports the error or creates the file. A dangling
/// symlink resolves to its target, which `std::fs::write` would create.
fn resolve_final_symlinks(path: &Path) -> io::Result<PathBuf> {
    let mut current = path.to_path_buf();
    for _ in 0..MAX_SYMLINK_HOPS {
        let (parent, name) = parent_and_name(&current)?;
        let dir = Dir::open_ambient_dir(parent, ambient_authority())?;
        match dir.symlink_metadata(name) {
            Ok(metadata) if metadata.is_symlink() => {
                let target = dir.read_link_contents(name)?;
                current = parent.join(target);
            }
            _ => return Ok(current),
        }
    }
    Err(io::Error::other(format!(
        "too many levels of symbolic links: {}",
        path.display()
    )))
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

#[cfg(test)]
mod tests {
    //! Tests for the capability-based file helpers.

    use std::io::ErrorKind;
    use std::path::Path;

    use super::{create_dir_all, parent_and_name, read_dir, read_to_string, write};

    #[test]
    fn a_file_round_trips_through_its_parent_directory() {
        let dir = tempfile::tempdir().expect("temporary directory");
        let path = dir.path().join("note.txt");

        write(&path, "hello").expect("write");

        assert_eq!(read_to_string(&path).expect("read"), "hello");
    }

    #[test]
    fn writing_truncates_an_existing_file() {
        let dir = tempfile::tempdir().expect("temporary directory");
        let path = dir.path().join("note.txt");
        write(&path, "a longer first version").expect("first write");

        write(&path, "short").expect("second write");

        assert_eq!(read_to_string(&path).expect("read"), "short");
    }

    #[test]
    fn a_missing_file_is_not_found() {
        let dir = tempfile::tempdir().expect("temporary directory");

        let error = read_to_string(dir.path().join("absent")).expect_err("absent file");

        assert_eq!(error.kind(), ErrorKind::NotFound);
    }

    #[test]
    fn a_missing_parent_directory_fails_both_helpers() {
        let dir = tempfile::tempdir().expect("temporary directory");
        let path = dir.path().join("no-such-dir").join("note.txt");

        assert_eq!(
            write(&path, "x").expect_err("no parent").kind(),
            ErrorKind::NotFound
        );
        assert_eq!(
            read_to_string(&path).expect_err("no parent").kind(),
            ErrorKind::NotFound
        );
    }

    #[test]
    fn nested_directories_are_created_and_creation_is_idempotent() {
        let dir = tempfile::tempdir().expect("temporary directory");
        let nested = dir.path().join("a").join("b").join("c");

        create_dir_all(&nested).expect("first creation");
        create_dir_all(&nested).expect("second creation");

        assert!(nested.is_dir());
    }

    #[test]
    fn invalid_utf8_contents_fail_the_read() {
        let dir = tempfile::tempdir().expect("temporary directory");
        let path = dir.path().join("bytes.bin");
        write(&path, [0xff, 0xfe]).expect("write");

        let error = read_to_string(&path).expect_err("not UTF-8");

        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn a_path_without_a_file_name_is_invalid_input() {
        let error = parent_and_name(Path::new("/")).expect_err("root has no file name");

        assert_eq!(error.kind(), ErrorKind::InvalidInput);
    }

    #[test]
    fn a_bare_file_name_is_resolved_against_the_current_directory() {
        let (parent, name) = parent_and_name(Path::new("note.txt")).expect("bare name");

        assert_eq!(parent, Path::new("."));
        assert_eq!(name, "note.txt");
    }

    #[test]
    fn a_directory_lists_its_entries_as_paths_beneath_it() {
        let dir = tempfile::tempdir().expect("temporary directory");
        write(dir.path().join("a.txt"), "a").expect("write a");
        create_dir_all(dir.path().join("sub")).expect("create sub");

        let mut entries = read_dir(dir.path()).expect("list");
        entries.sort();

        assert_eq!(
            entries,
            vec![dir.path().join("a.txt"), dir.path().join("sub")]
        );
    }

    #[test]
    fn listing_a_missing_directory_is_not_found() {
        let dir = tempfile::tempdir().expect("temporary directory");

        let error = read_dir(dir.path().join("absent")).expect_err("absent directory");

        assert_eq!(error.kind(), ErrorKind::NotFound);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_with_an_absolute_target_is_followed_for_reads_and_writes() {
        let dir = tempfile::tempdir().expect("temporary directory");
        let target = dir.path().join("target.txt");
        let link = dir.path().join("link.txt");
        write(&target, "original").expect("write target");
        std::os::unix::fs::symlink(&target, &link).expect("symlink");

        assert_eq!(
            read_to_string(&link).expect("read through link"),
            "original"
        );

        write(&link, "updated").expect("write through link");
        assert_eq!(read_to_string(&target).expect("read target"), "updated");
        assert!(link.is_symlink());
    }

    #[cfg(unix)]
    #[test]
    fn a_chain_of_relative_symlinks_in_other_directories_is_followed() {
        let dir = tempfile::tempdir().expect("temporary directory");
        create_dir_all(dir.path().join("a")).expect("create a");
        create_dir_all(dir.path().join("b")).expect("create b");
        write(dir.path().join("b").join("real.txt"), "real").expect("write real");
        std::os::unix::fs::symlink("../b/real.txt", dir.path().join("a").join("hop.txt"))
            .expect("first link");
        std::os::unix::fs::symlink("a/hop.txt", dir.path().join("start.txt")).expect("second link");

        assert_eq!(
            read_to_string(dir.path().join("start.txt")).expect("read"),
            "real"
        );
    }

    #[cfg(unix)]
    #[test]
    fn writing_through_a_dangling_symlink_creates_its_target() {
        let dir = tempfile::tempdir().expect("temporary directory");
        let target = dir.path().join("created.txt");
        let link = dir.path().join("dangling.txt");
        std::os::unix::fs::symlink(&target, &link).expect("symlink");

        write(&link, "made").expect("write through dangling link");

        assert_eq!(read_to_string(&target).expect("read target"), "made");
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_loop_fails_instead_of_hanging() {
        let dir = tempfile::tempdir().expect("temporary directory");
        let first = dir.path().join("first");
        let second = dir.path().join("second");
        std::os::unix::fs::symlink(&second, &first).expect("first link");
        std::os::unix::fs::symlink(&first, &second).expect("second link");

        let error = read_to_string(&first).expect_err("loop");

        assert!(error
            .to_string()
            .contains("too many levels of symbolic links"));
    }
}

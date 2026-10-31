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
//! One behaviour differs from `std::fs`: `cap-std` resolves symlinks within the
//! opened directory and refuses one whose target escapes it, including an
//! absolute target. A file that is itself such a symlink now fails, and the
//! callers treat that as they treat any other I/O error. Symlinks in the parent
//! path are resolved by the operating system as before.

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

/// Open the directory that holds `path` as a capability.
fn open_parent(path: &Path) -> io::Result<(Dir, &OsStr)> {
    let (parent, name) = parent_and_name(path)?;
    Ok((Dir::open_ambient_dir(parent, ambient_authority())?, name))
}

/// Read the whole file at `path` into a string.
///
/// # Errors
///
/// Returns the I/O error when the parent directory or the file cannot be
/// opened, or when the contents are not valid UTF-8.
pub fn read_to_string(path: impl AsRef<Path>) -> io::Result<String> {
    let (dir, name) = open_parent(path.as_ref())?;
    dir.read_to_string(name)
}

/// Write `contents` to the file at `path`, creating or truncating it.
///
/// # Errors
///
/// Returns the I/O error when the parent directory cannot be opened or the file
/// cannot be written.
pub fn write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> io::Result<()> {
    let (dir, name) = open_parent(path.as_ref())?;
    dir.write(name, contents)
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
}

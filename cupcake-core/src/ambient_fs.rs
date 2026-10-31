//! Ambient filesystem probes that cannot be scoped to a directory capability.
//!
//! The symlink resolver is a defence against TOB-4 (bypassing path policies
//! through symlinks): it must learn where an arbitrary, caller-supplied path
//! really points, however many directories the links cross. `cap-std` cannot
//! answer that. `cap-primitives` refuses any symlink whose target is absolute
//! or leaves the opened directory as an escape attempt, so a capability rooted
//! at `/` still fails on links such as `/usr/bin/cc -> /etc/alternatives/cc`,
//! and a resolver built on it would report no target for exactly the links an
//! attacker would use.
//!
//! This module holds only the three probes the resolver needs, each a thin
//! wrapper over the `std::fs` call it names, and is the only module in the
//! workspace that Whitaker's `no_std_fs_operations` lint exempts (see
//! `dylint.toml`). It reads nothing but link structure: file contents and
//! writes go through [`crate::fs_access`]. Do not add probes here without the
//! same justification.

use std::io;
use std::path::{Path, PathBuf};

/// Resolve `path` to an absolute path with every symlink followed.
///
/// # Errors
///
/// Returns the I/O error when the path or any link on the way does not exist or
/// cannot be read.
pub fn canonicalize(path: &Path) -> io::Result<PathBuf> {
    std::fs::canonicalize(path)
}

/// Report whether `path` is itself a symlink, without following it, so that a
/// dangling link is still detected.
///
/// # Errors
///
/// Returns the I/O error when the path cannot be inspected, for example when it
/// does not exist.
pub fn is_symlink(path: &Path) -> io::Result<bool> {
    Ok(std::fs::symlink_metadata(path)?.is_symlink())
}

/// Read the raw target of the symlink at `path`, without resolving it.
///
/// # Errors
///
/// Returns the I/O error when the path is not a symlink or cannot be read.
pub fn read_link(path: &Path) -> io::Result<PathBuf> {
    std::fs::read_link(path)
}

#[cfg(all(test, unix))]
mod tests {
    //! Tests that the probes follow the links `cap-std` would refuse.

    use std::os::unix::fs::symlink;

    use super::{canonicalize, is_symlink, read_link};
    use crate::fs_access;

    #[test]
    fn a_chain_of_absolute_links_across_directories_resolves_to_the_real_file() {
        // The shape of /usr/bin/cc -> /etc/alternatives/cc -> /usr/bin/gcc.
        let root = tempfile::tempdir().expect("temporary directory");
        let bin = root.path().join("bin");
        let alternatives = root.path().join("alternatives");
        let real = root.path().join("real");
        for dir in [&bin, &alternatives, &real] {
            fs_access::create_dir_all(dir).expect("create directory");
        }
        fs_access::write(real.join("gcc"), "compiler").expect("write target");
        symlink(real.join("gcc"), alternatives.join("cc")).expect("inner link");
        symlink(alternatives.join("cc"), bin.join("cc")).expect("outer link");

        let resolved = canonicalize(&bin.join("cc")).expect("canonicalize");

        assert_eq!(
            resolved,
            canonicalize(&real.join("gcc")).expect("canonical target")
        );
    }

    #[test]
    fn a_dangling_link_is_detected_and_its_raw_target_read() {
        let root = tempfile::tempdir().expect("temporary directory");
        let link = root.path().join("dangling");
        let missing = root.path().join("missing");
        symlink(&missing, &link).expect("dangling link");

        assert!(is_symlink(&link).expect("inspect link"));
        assert_eq!(read_link(&link).expect("read link"), missing);
        assert!(canonicalize(&link).is_err());
    }

    #[test]
    fn a_regular_file_is_not_a_symlink_and_a_missing_path_is_an_error() {
        let root = tempfile::tempdir().expect("temporary directory");
        let file = root.path().join("file");
        fs_access::write(&file, "x").expect("write file");

        assert!(!is_symlink(&file).expect("inspect file"));
        assert!(is_symlink(&root.path().join("absent")).is_err());
        assert!(read_link(&file).is_err());
    }
}

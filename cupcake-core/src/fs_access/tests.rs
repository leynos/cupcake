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

#[cfg(unix)]
#[test]
fn the_unix_mode_of_a_file_is_read_through_a_symlink() {
    use cap_std::ambient_authority;
    use cap_std::fs::{Dir, Permissions, PermissionsExt};

    let dir = tempfile::tempdir().expect("temporary directory");
    let tool = dir.path().join("tool");
    let link = dir.path().join("link");
    write(&tool, "#!/bin/sh\n").expect("write tool");
    Dir::open_ambient_dir(dir.path(), ambient_authority())
        .and_then(|root| root.set_permissions("tool", Permissions::from_mode(0o750)))
        .expect("set mode");
    std::os::unix::fs::symlink(&tool, &link).expect("symlink");

    assert_eq!(super::unix_mode(&tool).expect("mode") & 0o777, 0o750);
    assert_eq!(
        super::unix_mode(&link).expect("mode via link") & 0o777,
        0o750
    );
    assert_eq!(
        super::unix_mode(dir.path().join("absent"))
            .expect_err("absent")
            .kind(),
        ErrorKind::NotFound
    );
}

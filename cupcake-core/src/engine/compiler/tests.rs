//! Tests for the policy-root discovery helper.

use super::*;

fn unit(path: &str) -> PolicyUnit {
    PolicyUnit {
        path: PathBuf::from(path),
        package_name: "p".to_string(),
        routing: Default::default(),
        metadata: None,
    }
}

#[test]
fn root_is_the_policies_directory_above_the_first_policy() {
    let policies = [unit("/proj/.cupcake/policies/claude/sub/a.rego")];
    let root = find_policies_root(&policies).expect("root");
    assert_eq!(root, Path::new("/proj/.cupcake/policies"));
}

#[test]
fn root_falls_back_to_the_first_policys_parent() {
    let policies = [unit("/proj/rules/a.rego")];
    let root = find_policies_root(&policies).expect("root");
    assert_eq!(root, Path::new("/proj/rules"));
}

#[test]
fn a_policy_path_without_a_parent_is_an_error() {
    let error = find_policies_root(&[unit("/")]).expect_err("no parent");
    assert!(error.to_string().contains("no parent directory"));
}

#[test]
fn no_policies_is_an_error() {
    let error = find_policies_root(&[]).expect_err("no root");
    assert!(error.to_string().contains("No policies"));
}

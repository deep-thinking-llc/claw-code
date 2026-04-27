#![allow(clippy::doc_markdown, clippy::uninlined_format_args)]
use ninmu_runtime::permission_enforcer::{EnforcementResult, PermissionEnforcer};
use ninmu_runtime::{PermissionMode, PermissionPolicy};

fn enforcer(mode: PermissionMode) -> PermissionEnforcer {
    PermissionEnforcer::new(PermissionPolicy::new(mode))
}

fn is_allowed(result: &EnforcementResult) -> bool {
    matches!(result, EnforcementResult::Allowed)
}

#[test]
fn readonly_blocks_bash_write_commands() {
    let enf = enforcer(PermissionMode::ReadOnly);
    let result = enf.check_bash("touch /tmp/test");
    assert!(
        !is_allowed(&result),
        "ReadOnly should block touch, got: {result:?}"
    );
}

#[test]
fn readonly_allows_read_commands() {
    let enf = enforcer(PermissionMode::ReadOnly);
    let ls = enf.check_bash("ls -la");
    let cat = enf.check_bash("cat file.txt");
    assert!(is_allowed(&ls), "ReadOnly should allow ls: {ls:?}");
    assert!(is_allowed(&cat), "ReadOnly should allow cat: {cat:?}");
}

#[test]
fn workspace_write_allows_in_workspace() {
    let enf = enforcer(PermissionMode::WorkspaceWrite);
    let result = enf.check_file_write("src/main.rs", "/home/user/project");
    assert!(
        is_allowed(&result),
        "WorkspaceWrite should allow workspace files: {result:?}"
    );
}

#[test]
fn workspace_write_blocks_outside_workspace() {
    let enf = enforcer(PermissionMode::WorkspaceWrite);
    let result = enf.check_file_write("/etc/passwd", "/home/user/project");
    assert!(
        !is_allowed(&result),
        "WorkspaceWrite should block files outside workspace: {result:?}"
    );
}

#[test]
fn danger_full_access_allows_all() {
    let enf = enforcer(PermissionMode::DangerFullAccess);
    let bash = enf.check_bash("rm -rf /");
    let file = enf.check_file_write("/etc/passwd", "/home/user/project");
    assert!(
        is_allowed(&bash),
        "DangerFullAccess should allow any bash: {bash:?}"
    );
    assert!(
        is_allowed(&file),
        "DangerFullAccess should allow any file write: {file:?}"
    );
}

#[test]
fn active_mode_reports_correctly() {
    assert_eq!(
        enforcer(PermissionMode::ReadOnly).active_mode(),
        PermissionMode::ReadOnly
    );
    assert_eq!(
        enforcer(PermissionMode::WorkspaceWrite).active_mode(),
        PermissionMode::WorkspaceWrite
    );
    assert_eq!(
        enforcer(PermissionMode::DangerFullAccess).active_mode(),
        PermissionMode::DangerFullAccess
    );
}

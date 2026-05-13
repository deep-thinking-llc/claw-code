#![allow(clippy::doc_markdown, clippy::uninlined_format_args)]
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex as StdMutex, OnceLock};
use std::time::SystemTime;

use ninmu_runtime::{execute_bash, BashCommandInput, FilesystemIsolationMode};

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<StdMutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| StdMutex::new(()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn temp_dir(label: &str) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("ninmu-sandbox-test-{label}-{ts}"));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn cleanup(dir: &PathBuf) {
    let _ = fs::remove_dir_all(dir);
}

fn sandbox_input(command: &str) -> BashCommandInput {
    BashCommandInput {
        command: command.to_string(),
        timeout: Some(10_000),
        description: None,
        run_in_background: None,
        dangerously_disable_sandbox: Some(false),
        namespace_restrictions: None,
        isolate_network: None,
        filesystem_mode: Some(FilesystemIsolationMode::WorkspaceOnly),
        allowed_mounts: None,
    }
}

fn unsandbox_input(command: &str) -> BashCommandInput {
    BashCommandInput {
        command: command.to_string(),
        timeout: Some(10_000),
        description: None,
        run_in_background: None,
        dangerously_disable_sandbox: Some(true),
        namespace_restrictions: None,
        isolate_network: None,
        filesystem_mode: None,
        allowed_mounts: None,
    }
}

#[test]
fn home_dir_redirected_in_sandbox() {
    let _lock = env_lock();
    let dir = temp_dir("home-redirect");
    let real_home = std::env::var("HOME").unwrap_or_default();

    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(&dir).unwrap();

    let result = execute_bash(sandbox_input("echo $HOME")).unwrap();

    std::env::set_current_dir(&original).unwrap();

    let home_output = result.stdout.trim();
    assert_ne!(home_output, real_home, "HOME should differ from real HOME");
    assert!(
        home_output.contains(".sandbox-home"),
        "HOME should contain .sandbox-home, got: {home_output}"
    );

    cleanup(&dir);
}

#[test]
fn tmpdir_redirected_in_sandbox() {
    let _lock = env_lock();
    let dir = temp_dir("tmpdir-redirect");

    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(&dir).unwrap();

    let result = execute_bash(sandbox_input("echo $TMPDIR")).unwrap();

    std::env::set_current_dir(&original).unwrap();

    let tmpdir_output = result.stdout.trim();
    assert!(
        tmpdir_output.contains(".sandbox-tmp"),
        "TMPDIR should contain .sandbox-tmp, got: {tmpdir_output}"
    );

    cleanup(&dir);
}

#[test]
fn sandbox_dirs_created_automatically() {
    let _lock = env_lock();
    let dir = temp_dir("auto-dirs");

    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(&dir).unwrap();

    let _ = execute_bash(sandbox_input("echo created"));

    std::env::set_current_dir(&original).unwrap();

    assert!(
        dir.join(".sandbox-home").is_dir(),
        ".sandbox-home should be created"
    );
    assert!(
        dir.join(".sandbox-tmp").is_dir(),
        ".sandbox-tmp should be created"
    );

    cleanup(&dir);
}

#[test]
fn sandbox_disabled_allows_real_home() {
    let _lock = env_lock();
    let dir = temp_dir("no-sandbox");
    let real_home = std::env::var("HOME").unwrap_or_default();

    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(&dir).unwrap();

    let result = execute_bash(unsandbox_input("echo $HOME")).unwrap();

    std::env::set_current_dir(&original).unwrap();

    let home_output = result.stdout.trim();
    assert_eq!(
        home_output, real_home,
        "HOME should be the real HOME when sandbox is disabled"
    );

    cleanup(&dir);
}

#[test]
fn write_to_workspace_succeeds_in_sandbox() {
    let _lock = env_lock();
    let dir = temp_dir("workspace-write");

    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(&dir).unwrap();

    let result = execute_bash(sandbox_input("touch testfile.txt")).unwrap();
    assert!(!result.interrupted, "should not be interrupted");

    std::env::set_current_dir(&original).unwrap();

    assert!(
        dir.join("testfile.txt").is_file(),
        "testfile.txt should be created"
    );

    cleanup(&dir);
}

#[test]
fn output_truncation_at_16kib() {
    let _lock = env_lock();
    let dir = temp_dir("truncate");

    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(&dir).unwrap();

    let command = "python3 -c \"import sys; sys.stdout.write('A' * 20000)\"";
    let result = execute_bash(sandbox_input(command)).unwrap();

    std::env::set_current_dir(&original).unwrap();

    assert!(result.stdout.len() < 20_000, "output should be truncated");
    assert!(
        result.stdout.contains("truncated") || result.stdout.len() <= 16_384 + 100,
        "output should indicate truncation or be near 16KiB limit, got {} bytes",
        result.stdout.len()
    );

    cleanup(&dir);
}

#[test]
fn command_timeout_is_enforced() {
    let _lock = env_lock();
    let dir = temp_dir("timeout");

    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(&dir).unwrap();

    let input = BashCommandInput {
        command: "sleep 10".to_string(),
        timeout: Some(500),
        description: None,
        run_in_background: None,
        dangerously_disable_sandbox: Some(true),
        namespace_restrictions: None,
        isolate_network: None,
        filesystem_mode: None,
        allowed_mounts: None,
    };
    let result = execute_bash(input).unwrap();

    std::env::set_current_dir(&original).unwrap();

    assert!(result.interrupted, "should be interrupted by timeout");
    assert_eq!(
        result.return_code_interpretation.as_deref(),
        Some("timeout"),
        "return code should indicate timeout"
    );

    cleanup(&dir);
}

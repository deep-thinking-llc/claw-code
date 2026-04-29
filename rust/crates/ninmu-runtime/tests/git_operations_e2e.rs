#![allow(clippy::doc_markdown, clippy::uninlined_format_args)]
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex as StdMutex, OnceLock};
use std::time::SystemTime;

use ninmu_runtime::{
    check_base_commit, format_stale_base_warning, read_claw_base_file, BaseCommitSource,
    BaseCommitState, GitContext,
};

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
    let dir = std::env::temp_dir().join(format!("ninmu-git-e2e-{label}-{ts}"));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn cleanup(dir: &PathBuf) {
    let _ = fs::remove_dir_all(dir);
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@test.com")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@test.com")
        .status()
        .unwrap();
    assert!(status.success(), "git {:?} failed", args);
}

fn init_repo(dir: &Path) {
    git(dir, &["init"]);
    git(dir, &["config", "user.name", "test"]);
    git(dir, &["config", "user.email", "test@test.com"]);
}

fn commit_file(dir: &Path, name: &str, content: &str) {
    fs::write(dir.join(name), content).unwrap();
    git(dir, &["add", name]);
    git(dir, &["commit", "-m", &format!("add {name}")]);
}

#[test]
fn detect_returns_none_for_non_git_directory() {
    let dir = temp_dir("non-git");
    assert!(GitContext::detect(&dir).is_none());
    cleanup(&dir);
}

#[test]
fn detect_in_subdirectory_finds_parent_repo() {
    let _lock = env_lock();
    let dir = temp_dir("subdir");
    init_repo(&dir);
    commit_file(&dir, "root.txt", "hello");

    let subdir = dir.join("sub");
    fs::create_dir_all(&subdir).unwrap();
    let ctx = GitContext::detect(&subdir).expect("should detect from subdir");
    assert!(ctx.branch.is_some(), "branch should be detected");

    cleanup(&dir);
}

#[test]
fn commit_limit_is_five() {
    let _lock = env_lock();
    let dir = temp_dir("commit-limit");
    init_repo(&dir);

    for i in 0..10 {
        commit_file(&dir, &format!("file{i}.txt"), &format!("content{i}"));
    }

    let ctx = GitContext::detect(&dir).expect("should detect");
    assert_eq!(ctx.recent_commits.len(), 5, "should limit to 5 commits");

    cleanup(&dir);
}

#[test]
fn staged_files_detected() {
    let _lock = env_lock();
    let dir = temp_dir("staged");
    init_repo(&dir);
    commit_file(&dir, "initial.txt", "first");

    fs::write(dir.join("new.txt"), "staged content").unwrap();
    git(&dir, &["add", "new.txt"]);

    let ctx = GitContext::detect(&dir).expect("should detect");
    assert!(
        ctx.staged_files.contains(&"new.txt".to_string()),
        "new.txt should be staged"
    );

    cleanup(&dir);
}

#[test]
fn detached_head_context() {
    let _lock = env_lock();
    let dir = temp_dir("detached");
    init_repo(&dir);
    commit_file(&dir, "a.txt", "first");
    commit_file(&dir, "b.txt", "second");

    let output = Command::new("git")
        .current_dir(&dir)
        .args(["rev-parse", "HEAD~1"])
        .output()
        .unwrap();
    let first_commit = String::from_utf8_lossy(&output.stdout).trim().to_string();
    git(&dir, &["checkout", &first_commit]);

    let ctx = GitContext::detect(&dir);
    assert!(ctx.is_some(), "should still detect in detached HEAD");

    git(&dir, &["checkout", "-"]); // restore
    cleanup(&dir);
}

#[test]
fn render_with_empty_repo() {
    let _lock = env_lock();
    let dir = temp_dir("empty-repo");
    init_repo(&dir);

    let ctx = GitContext::detect(&dir);
    // Empty repo (no commits) should not panic on render
    if let Some(ctx) = ctx {
        let _rendered = ctx.render();
    }

    cleanup(&dir);
}

#[test]
fn check_base_commit_detects_diverged() {
    let _lock = env_lock();
    let dir = temp_dir("stale-base");
    init_repo(&dir);
    commit_file(&dir, "initial.txt", "first");

    let output = Command::new("git")
        .current_dir(&dir)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    let base_sha = String::from_utf8_lossy(&output.stdout).trim().to_string();

    commit_file(&dir, "second.txt", "second");
    let output2 = Command::new("git")
        .current_dir(&dir)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    let new_sha = String::from_utf8_lossy(&output2.stdout).trim().to_string();

    let source = BaseCommitSource::Flag(base_sha.clone());
    let state = check_base_commit(&dir, Some(&source));

    match state {
        BaseCommitState::Diverged { .. } => {}
        other => panic!("expected Diverged, got {:?}", other),
    }

    let warning = format_stale_base_warning(&state);
    assert!(
        warning.is_some(),
        "should produce a warning for diverged state"
    );
    let warning_text = warning.unwrap();
    assert!(
        warning_text.contains(&base_sha[..7]) || warning_text.contains(&new_sha[..7]),
        "warning should contain commit hash, got: {warning_text}"
    );

    cleanup(&dir);
}

#[test]
fn check_base_commit_matches_when_unchanged() {
    let _lock = env_lock();
    let dir = temp_dir("base-match");
    init_repo(&dir);
    commit_file(&dir, "initial.txt", "first");

    let output = Command::new("git")
        .current_dir(&dir)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    let base_sha = String::from_utf8_lossy(&output.stdout).trim().to_string();

    let source = BaseCommitSource::Flag(base_sha);
    let state = check_base_commit(&dir, Some(&source));
    assert_eq!(
        state,
        BaseCommitState::Matches,
        "should match when unchanged"
    );

    let warning = format_stale_base_warning(&state);
    assert!(warning.is_none(), "no warning when base matches");

    cleanup(&dir);
}

#[test]
fn read_claw_base_file_returns_content() {
    let _lock = env_lock();
    let dir = temp_dir("claw-base-file");
    init_repo(&dir);
    commit_file(&dir, "initial.txt", "first");

    let output = Command::new("git")
        .current_dir(&dir)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();

    fs::write(dir.join(".ninmu-base"), &sha).unwrap();

    let result = read_claw_base_file(&dir);
    assert_eq!(result, Some(sha));

    cleanup(&dir);
}

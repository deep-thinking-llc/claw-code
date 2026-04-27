#![allow(clippy::doc_markdown, clippy::uninlined_format_args)]
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

use ninmu_runtime::{ConversationMessage, Session, SessionStore};

fn temp_dir(label: &str) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("ninmu-session-e2e-{label}-{ts}"));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn cleanup(dir: &PathBuf) {
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn create_push_messages_save_load_delete_round_trip() {
    let dir = temp_dir("round-trip");
    let path = dir.join("session.jsonl");

    let mut session = Session::new()
        .with_persistence_path(&path)
        .with_workspace_root(&dir);

    for i in 0..20 {
        session
            .push_message(ConversationMessage::user_text(format!("message {i}")))
            .unwrap();
    }

    assert_eq!(session.messages.len(), 20);
    session.save_to_path(&path).unwrap();

    let loaded = Session::load_from_path(&path).unwrap();
    assert_eq!(loaded.messages.len(), 20);
    assert_eq!(loaded.session_id, session.session_id);

    fs::remove_file(&path).unwrap();
    assert!(!path.exists(), "file should be deleted");
    cleanup(&dir);
}

#[test]
fn jsonl_format_contains_all_record_types() {
    let dir = temp_dir("jsonl-types");
    let path = dir.join("session.jsonl");

    let mut session = Session::new().with_persistence_path(&path);

    session
        .push_message(ConversationMessage::user_text("user msg"))
        .unwrap();
    session.record_compaction("compacted summary", 5);

    session.save_to_path(&path).unwrap();

    let raw = fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = raw.lines().collect();
    assert!(
        lines.len() >= 3,
        "should have meta, compaction, and message lines"
    );

    let types: Vec<String> = lines
        .iter()
        .filter_map(|line| {
            let val: serde_json::Value = serde_json::from_str(line).ok()?;
            val.get("type")?.as_str().map(String::from)
        })
        .collect();

    assert!(
        types.contains(&"session_meta".to_string()),
        "should have session_meta"
    );
    assert!(
        types.contains(&"compaction".to_string()),
        "should have compaction"
    );
    assert!(
        types.contains(&"message".to_string()),
        "should have message"
    );

    cleanup(&dir);
}

#[test]
fn incremental_append_vs_full_snapshot_no_duplicates() {
    let dir = temp_dir("no-dupes");
    let path = dir.join("session.jsonl");

    let mut session = Session::new().with_persistence_path(&path);

    for i in 0..5 {
        session
            .push_message(ConversationMessage::user_text(format!("msg {i}")))
            .unwrap();
    }

    assert_eq!(session.messages.len(), 5);
    let raw = fs::read_to_string(&path).unwrap();
    let _incremental_count = raw.lines().count();

    session.save_to_path(&path).unwrap();
    let raw_after = fs::read_to_string(&path).unwrap();
    let snapshot_lines: Vec<&str> = raw_after.lines().collect();

    let msg_lines: Vec<&&str> = snapshot_lines
        .iter()
        .filter(|line| line.contains("\"message\""))
        .collect();
    assert_eq!(
        msg_lines.len(),
        5,
        "should have exactly 5 message lines after snapshot"
    );

    cleanup(&dir);
}

#[test]
fn fork_preserves_parent_lineage() {
    let dir = temp_dir("lineage");
    let path_parent = dir.join("parent.jsonl");
    let path_child = dir.join("child.jsonl");
    let path_grandchild = dir.join("grandchild.jsonl");

    let mut parent = Session::new().with_persistence_path(&path_parent);
    parent
        .push_message(ConversationMessage::user_text("parent msg"))
        .unwrap();
    parent.save_to_path(&path_parent).unwrap();
    let parent_id = parent.session_id.clone();

    let child = parent.fork(Some("feature-branch".to_string()));
    assert_eq!(
        child.fork.as_ref().unwrap().parent_session_id,
        parent_id,
        "child should reference parent"
    );
    let mut child = child.with_persistence_path(&path_child);
    child
        .push_message(ConversationMessage::user_text("child msg"))
        .unwrap();
    child.save_to_path(&path_child).unwrap();
    let child_id = child.session_id.clone();

    let grandchild = child.fork(None);
    assert_eq!(
        grandchild.fork.as_ref().unwrap().parent_session_id,
        child_id,
        "grandchild should reference child"
    );
    let grandchild = grandchild.with_persistence_path(&path_grandchild);
    assert_eq!(
        grandchild.fork.as_ref().unwrap().parent_session_id,
        child_id
    );

    cleanup(&dir);
}

#[test]
fn concurrent_sessions_isolated() {
    let dir_a = temp_dir("iso-a");
    let dir_b = temp_dir("iso-b");
    let path_a = dir_a.join("session-a.jsonl");
    let path_b = dir_b.join("session-b.jsonl");

    let mut session_a = Session::new().with_persistence_path(&path_a);
    let mut session_b = Session::new().with_persistence_path(&path_b);

    session_a
        .push_message(ConversationMessage::user_text("msg for A"))
        .unwrap();
    session_b
        .push_message(ConversationMessage::user_text("msg for B"))
        .unwrap();

    assert_ne!(
        session_a.session_id, session_b.session_id,
        "IDs should differ"
    );
    assert_eq!(session_a.messages.len(), 1);
    assert_eq!(session_b.messages.len(), 1);

    let loaded_a = Session::load_from_path(&path_a).unwrap();
    let loaded_b = Session::load_from_path(&path_b).unwrap();
    assert_eq!(loaded_a.messages.len(), 1);
    assert_eq!(loaded_b.messages.len(), 1);
    assert_ne!(loaded_a.session_id, loaded_b.session_id);

    cleanup(&dir_a);
    cleanup(&dir_b);
}

#[test]
fn workspace_root_persists_round_trip() {
    let dir = temp_dir("workspace-root");
    let path = dir.join("session.jsonl");

    let session = Session::new()
        .with_persistence_path(&path)
        .with_workspace_root(&dir);
    session.save_to_path(&path).unwrap();

    let loaded = Session::load_from_path(&path).unwrap();
    assert_eq!(
        loaded.workspace_root(),
        Some(dir.as_path()),
        "workspace root should round-trip"
    );

    cleanup(&dir);
}

#[test]
fn session_id_uniqueness_under_tight_loop() {
    let mut ids = std::collections::HashSet::new();
    for _ in 0..100 {
        let session = Session::new();
        assert!(
            ids.insert(session.session_id.clone()),
            "session ID should be unique"
        );
    }
}

#[test]
fn session_store_create_list_load_round_trip() {
    let dir = temp_dir("store-roundtrip");

    let store = SessionStore::from_cwd(&dir).unwrap();
    let handle = store.create_handle("test-session");
    let mut session = Session::new().with_persistence_path(&handle.path);
    session
        .push_message(ConversationMessage::user_text("stored msg"))
        .unwrap();
    session.save_to_path(&handle.path).unwrap();

    let sessions = store.list_sessions().unwrap();
    assert_eq!(sessions.len(), 1, "should list 1 session");

    let loaded = Session::load_from_path(&handle.path).unwrap();
    assert_eq!(loaded.messages.len(), 1);

    cleanup(&dir);
}

#[test]
fn compaction_metadata_round_trips() {
    let dir = temp_dir("compaction");
    let path = dir.join("session.jsonl");

    let mut session = Session::new().with_persistence_path(&path);
    session
        .push_message(ConversationMessage::user_text("before compaction"))
        .unwrap();
    session.record_compaction("summary of old messages", 10);

    session.save_to_path(&path).unwrap();

    let loaded = Session::load_from_path(&path).unwrap();
    let compaction = loaded
        .compaction
        .as_ref()
        .expect("compaction should be present");
    assert_eq!(compaction.summary, "summary of old messages");
    assert_eq!(compaction.removed_message_count, 10);

    cleanup(&dir);
}

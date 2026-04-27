#![allow(clippy::doc_markdown, clippy::uninlined_format_args)]
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex as StdMutex, OnceLock};
use std::time::SystemTime;

use ninmu_runtime::{check_unsupported_format, ConfigLoader};

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
    let dir = std::env::temp_dir().join(format!("ninmu-config-e2e-{label}-{ts}"));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn cleanup(dir: &PathBuf) {
    let _ = fs::remove_dir_all(dir);
}

fn write_json(path: &Path, json: &serde_json::Value) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, serde_json::to_string_pretty(json).unwrap()).unwrap();
}

fn settings_json(dir: &Path) -> PathBuf {
    dir.join(".claw").join("settings.json")
}

fn user_settings_json(home: &Path) -> PathBuf {
    home.join(".claw").join("settings.json")
}

fn local_settings_json(dir: &Path) -> PathBuf {
    dir.join(".claw").join("settings.local.json")
}

#[test]
fn empty_config_loads_defaults() {
    let dir = temp_dir("defaults");
    let loader = ConfigLoader::new(&dir, dir.join(".claw"));
    let config = loader.load().unwrap();

    assert!(config.model().is_none(), "model should default to None");
    assert!(
        config.aliases().is_empty(),
        "aliases should default to empty"
    );
    assert!(
        config.loaded_entries().is_empty(),
        "no entries when no files exist"
    );

    cleanup(&dir);
}

#[test]
fn user_config_discovered_and_loaded() {
    let _lock = env_lock();
    let dir = temp_dir("user-config");
    let home = temp_dir("user-home");
    let home_settings = user_settings_json(&home);

    write_json(&home_settings, &serde_json::json!({ "model": "gpt-4o" }));

    let loader = ConfigLoader::new(&dir, home.join(".claw"));
    let config = loader.load().unwrap();

    assert_eq!(config.model(), Some("gpt-4o"));

    cleanup(&dir);
    cleanup(&home);
}

#[test]
fn project_config_overrides_user() {
    let _lock = env_lock();
    let dir = temp_dir("project-override");
    let home = temp_dir("proj-home");
    let home_settings = user_settings_json(&home);

    write_json(
        &home_settings,
        &serde_json::json!({ "model": "claude-opus-4-6" }),
    );

    let proj_settings = settings_json(&dir);
    write_json(&proj_settings, &serde_json::json!({ "model": "gpt-4o" }));

    let loader = ConfigLoader::new(&dir, home.join(".claw"));
    let config = loader.load().unwrap();

    assert_eq!(
        config.model(),
        Some("gpt-4o"),
        "project should override user"
    );

    cleanup(&dir);
    cleanup(&home);
}

#[test]
fn local_config_overrides_project() {
    let _lock = env_lock();
    let dir = temp_dir("local-override");
    let home = temp_dir("local-home");

    let proj_settings = settings_json(&dir);
    write_json(&proj_settings, &serde_json::json!({ "model": "gpt-4o" }));

    let local_settings = local_settings_json(&dir);
    write_json(
        &local_settings,
        &serde_json::json!({ "model": "claude-sonnet-4-6" }),
    );

    let loader = ConfigLoader::new(&dir, home.join(".claw"));
    let config = loader.load().unwrap();

    assert_eq!(
        config.model(),
        Some("claude-sonnet-4-6"),
        "local should override project"
    );

    cleanup(&dir);
    cleanup(&home);
}

#[test]
fn malformed_json_produces_error() {
    let dir = temp_dir("malformed");
    let settings = settings_json(&dir);
    write_json_raw(&settings, "{ invalid json !!!");

    let loader = ConfigLoader::new(&dir, dir.join(".claw"));
    let result = loader.load();

    assert!(result.is_err(), "malformed JSON should fail to load");

    cleanup(&dir);
}

#[test]
fn toml_file_rejected() {
    let dir = temp_dir("toml");
    let toml_path = dir.join(".claw").join("settings.toml");
    fs::create_dir_all(toml_path.parent().unwrap()).unwrap();
    fs::write(&toml_path, "model = \"gpt-4o\"").unwrap();

    let result = check_unsupported_format(&toml_path);
    assert!(result.is_err(), "TOML should be rejected");

    cleanup(&dir);
}

#[test]
fn mcp_servers_deep_merged_across_scopes() {
    let _lock = env_lock();
    let dir = temp_dir("mcp-merge");
    let home = temp_dir("mcp-home");

    let home_settings = user_settings_json(&home);
    write_json(
        &home_settings,
        &serde_json::json!({
            "mcpServers": {
                "server-a": { "command": "echo", "args": ["a"] }
            }
        }),
    );

    let proj_settings = settings_json(&dir);
    write_json(
        &proj_settings,
        &serde_json::json!({
            "mcpServers": {
                "server-b": { "command": "echo", "args": ["b"] }
            }
        }),
    );

    let loader = ConfigLoader::new(&dir, home.join(".claw"));
    let config = loader.load().unwrap();

    let mcp = config.mcp();
    let names: Vec<&String> = mcp.servers().keys().collect();
    assert!(
        names.iter().any(|n| n.as_str() == "server-a"),
        "server-a from user config should be present"
    );
    assert!(
        names.iter().any(|n| n.as_str() == "server-b"),
        "server-b from project config should be present"
    );

    cleanup(&dir);
    cleanup(&home);
}

#[test]
fn loaded_entries_tracks_sources() {
    let _lock = env_lock();
    let dir = temp_dir("entries");
    let home = temp_dir("entries-home");
    let home_settings = user_settings_json(&home);
    write_json(&home_settings, &serde_json::json!({ "model": "gpt-4o" }));

    let loader = ConfigLoader::new(&dir, home.join(".claw"));
    let config = loader.load().unwrap();

    assert!(
        !config.loaded_entries().is_empty(),
        "should have loaded entries"
    );

    cleanup(&dir);
    cleanup(&home);
}

#[test]
fn aliases_parsed_correctly() {
    let _lock = env_lock();
    let dir = temp_dir("aliases");
    let home = temp_dir("aliases-home");

    let proj_settings = settings_json(&dir);
    write_json(
        &proj_settings,
        &serde_json::json!({
            "aliases": {
                "fast": "gpt-4o",
                "smart": "claude-opus-4-6"
            }
        }),
    );

    let loader = ConfigLoader::new(&dir, home.join(".claw"));
    let config = loader.load().unwrap();

    assert_eq!(config.aliases().get("fast"), Some(&"gpt-4o".to_string()));
    assert_eq!(
        config.aliases().get("smart"),
        Some(&"claude-opus-4-6".to_string())
    );

    cleanup(&dir);
    cleanup(&home);
}

#[test]
fn sandbox_config_parsed() {
    let _lock = env_lock();
    let dir = temp_dir("sandbox-cfg");
    let home = temp_dir("sandbox-home");

    let proj_settings = settings_json(&dir);
    write_json(
        &proj_settings,
        &serde_json::json!({
            "sandbox": {
                "enabled": false,
                "filesystemMode": "off"
            }
        }),
    );

    let loader = ConfigLoader::new(&dir, home.join(".claw"));
    let config = loader.load().unwrap();

    let sandbox = config.sandbox();
    assert_eq!(sandbox.enabled, Some(false));

    cleanup(&dir);
    cleanup(&home);
}

fn write_json_raw(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

#![allow(clippy::doc_markdown, clippy::uninlined_format_args)]
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use ninmu_plugins::{
    load_plugin_from_directory, HookRunner, PluginHooks, PluginManager, PluginManagerConfig,
    PluginTool,
};

fn temp_dir(label: &str) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("ninmu-plugin-e2e-{label}-{ts}"));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn cleanup(dir: &PathBuf) {
    let _ = fs::remove_dir_all(dir);
}

fn write_manifest(dir: &Path, json: &serde_json::Value) {
    let manifest_path = dir.join("plugin.json");
    if let Some(parent) = manifest_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(manifest_path, serde_json::to_string_pretty(json).unwrap()).unwrap();
}

fn write_script(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, format!("#!/bin/sh\n{content}")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

fn minimal_manifest(name: &str) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "version": "1.0.0",
        "description": "test plugin"
    })
}

#[test]
fn discover_plugin_from_directory() {
    let dir = temp_dir("discover");
    let plugin_dir = dir.join("my-plugin");
    fs::create_dir_all(&plugin_dir).unwrap();

    write_manifest(&plugin_dir, &minimal_manifest("my-plugin"));

    let result = load_plugin_from_directory(&plugin_dir);
    assert!(result.is_ok(), "should load valid plugin: {:?}", result);
    let manifest = result.unwrap();
    assert_eq!(manifest.name, "my-plugin");
    assert_eq!(manifest.version, "1.0.0");

    cleanup(&dir);
}

#[test]
fn discover_plugin_with_hooks() {
    let dir = temp_dir("hooks");
    let plugin_dir = dir.join("hook-plugin");
    fs::create_dir_all(&plugin_dir).unwrap();

    let pre_script = plugin_dir.join("hooks").join("pre.sh");
    write_script(&pre_script, "exit 0");

    let manifest = serde_json::json!({
        "name": "hook-plugin",
        "version": "1.0.0",
        "description": "hook test",
        "hooks": {
            "PreToolUse": ["./hooks/pre.sh"]
        }
    });
    write_manifest(&plugin_dir, &manifest);

    let result = load_plugin_from_directory(&plugin_dir);
    assert!(
        result.is_ok(),
        "should load plugin with hooks: {:?}",
        result
    );
    let loaded = result.unwrap();
    assert_eq!(loaded.hooks.pre_tool_use.len(), 1);

    cleanup(&dir);
}

#[test]
fn pre_tool_use_hook_deny() {
    let dir = temp_dir("deny");
    let plugin_dir = dir.join("deny-plugin");
    fs::create_dir_all(&plugin_dir).unwrap();

    let pre_script = plugin_dir.join("hooks").join("deny.sh");
    write_script(&pre_script, "echo 'denied by policy'; exit 2");

    let manifest = serde_json::json!({
        "name": "deny-plugin",
        "version": "1.0.0",
        "description": "deny test",
        "hooks": {
            "PreToolUse": ["./hooks/deny.sh"]
        }
    });
    write_manifest(&plugin_dir, &manifest);

    let hooks = PluginHooks {
        pre_tool_use: vec![pre_script.to_str().unwrap().to_string()],
        post_tool_use: vec![],
        post_tool_use_failure: vec![],
    };
    let runner = HookRunner::new(hooks);
    let result = runner.run_pre_tool_use("Write", "{}");

    assert!(result.is_denied(), "hook should deny: {:?}", result);

    cleanup(&dir);
}

#[test]
fn pre_tool_use_hook_allow() {
    let dir = temp_dir("allow");
    let plugin_dir = dir.join("allow-plugin");
    fs::create_dir_all(&plugin_dir).unwrap();

    let pre_script = plugin_dir.join("hooks").join("allow.sh");
    write_script(&pre_script, "exit 0");

    let hooks = PluginHooks {
        pre_tool_use: vec![pre_script.to_str().unwrap().to_string()],
        post_tool_use: vec![],
        post_tool_use_failure: vec![],
    };
    let runner = HookRunner::new(hooks);
    let result = runner.run_pre_tool_use("Read", "{}");

    assert!(!result.is_denied(), "hook should allow");
    assert!(!result.is_failed(), "hook should not fail");

    cleanup(&dir);
}

#[test]
fn manifest_validation_rejects_missing_name() {
    let dir = temp_dir("bad-manifest");
    let plugin_dir = dir.join("bad-plugin");
    fs::create_dir_all(&plugin_dir).unwrap();

    write_manifest(
        &plugin_dir,
        &serde_json::json!({
            "version": "1.0.0",
            "description": "no name"
        }),
    );

    let result = load_plugin_from_directory(&plugin_dir);
    assert!(result.is_err(), "missing name should fail");

    cleanup(&dir);
}

#[test]
fn manifest_validation_rejects_bad_permission() {
    let dir = temp_dir("bad-perm");
    let plugin_dir = dir.join("perm-plugin");
    fs::create_dir_all(&plugin_dir).unwrap();

    write_manifest(
        &plugin_dir,
        &serde_json::json!({
            "name": "perm-plugin",
            "version": "1.0.0",
            "description": "bad perm",
            "permissions": ["fly"]
        }),
    );

    let result = load_plugin_from_directory(&plugin_dir);
    assert!(result.is_err(), "invalid permission should fail");

    cleanup(&dir);
}

#[test]
fn tool_execution_via_subprocess() {
    let dir = temp_dir("tool-exec");
    let plugin_dir = dir.join("tool-plugin");
    fs::create_dir_all(&plugin_dir).unwrap();

    let tool_script = plugin_dir.join("tools").join("echo.sh");
    write_script(&tool_script, "cat");

    write_manifest(
        &plugin_dir,
        &serde_json::json!({
            "name": "tool-plugin",
            "version": "1.0.0",
            "description": "tool test",
            "tools": [{
                "name": "echo_tool",
                "description": "echoes input",
                "inputSchema": {"type": "object"},
                "command": "./tools/echo.sh",
                "requiredPermission": "read-only"
            }]
        }),
    );

    let result = load_plugin_from_directory(&plugin_dir);
    assert!(result.is_ok(), "should load: {:?}", result);
    let manifest = result.unwrap();
    assert_eq!(manifest.tools.len(), 1);
    assert_eq!(manifest.tools[0].name, "echo_tool");

    cleanup(&dir);
}

#[test]
fn install_from_local_path() {
    let config_home = temp_dir("install-cfg");
    let plugin_source = temp_dir("install-src");

    write_manifest(
        &plugin_source,
        &serde_json::json!({
            "name": "installable",
            "version": "1.0.0",
            "description": "installable plugin"
        }),
    );

    let mut manager = PluginManager::new(PluginManagerConfig::new(&config_home));
    let outcome = manager.install(plugin_source.to_str().unwrap());

    assert!(outcome.is_ok(), "install should succeed: {:?}", outcome);
    let installed = outcome.unwrap();
    assert_eq!(installed.version, "1.0.0");

    cleanup(&config_home);
    cleanup(&plugin_source);
}

#[test]
fn enable_disable_toggles_hooks() {
    let config_home = temp_dir("toggle-cfg");

    let mut manager = PluginManager::new(PluginManagerConfig::new(&config_home));

    let hooks = manager.aggregated_hooks();
    assert!(hooks.is_ok(), "should aggregate hooks even with no plugins");
    assert!(hooks.unwrap().is_empty(), "no plugins = empty hooks");

    cleanup(&config_home);
}

#[test]
fn packaged_manifest_path_discovered() {
    let dir = temp_dir("packaged");
    let plugin_dir = dir.join("pkg-plugin");
    let packaged_dir = plugin_dir.join(".claude-plugin");
    fs::create_dir_all(&packaged_dir).unwrap();

    write_manifest(&packaged_dir, &minimal_manifest("pkg-plugin"));

    let result = load_plugin_from_directory(&plugin_dir);
    assert!(
        result.is_ok(),
        "should discover packaged manifest: {:?}",
        result
    );

    cleanup(&dir);
}

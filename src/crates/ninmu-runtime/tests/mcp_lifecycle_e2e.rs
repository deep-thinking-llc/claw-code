#![allow(clippy::doc_markdown, clippy::uninlined_format_args, unused_imports)]
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use ninmu_runtime::{
    ConfigSource, McpServerConfig, McpServerManager, McpServerManagerError, McpStdioServerConfig,
    ScopedMcpServerConfig,
};
use serde_json::json;
use tokio::runtime::Builder;

fn temp_dir() -> PathBuf {
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("mcp-e2e-{nanos}-{id}"))
}

fn write_mcp_server_script(label: &str) -> PathBuf {
    let root = temp_dir();
    fs::create_dir_all(&root).unwrap();
    let script_path = root.join(format!("mcp-{label}.py"));
    let script = [
        "#!/usr/bin/env python3",
        "import json, os, sys, time",
        "LABEL = os.environ.get('MCP_SERVER_LABEL', 'server')",
        "DELAY = int(os.environ.get('MCP_TOOL_CALL_DELAY_MS', '0'))",
        "EXIT_AFTER = os.environ.get('MCP_EXIT_AFTER_TOOLS_LIST') == '1'",
        "FAIL_ONCE = os.environ.get('MCP_FAIL_ONCE_MODE')",
        "FAIL_MARKER = os.environ.get('MCP_FAIL_ONCE_MARKER')",
        "TEST_VAR = os.environ.get('TEST_VAR', '')",
        "init_count = 0",
        "def fail_once():",
        "    if not FAIL_ONCE or not FAIL_MARKER: return False",
        "    if os.path.exists(FAIL_MARKER): return False",
        "    open(FAIL_MARKER,'w').write(FAIL_ONCE); return True",
        "def read_msg():",
        "    h = b''",
        r"    while not h.endswith(b'\r\n\r\n'):",
        "        c = sys.stdin.buffer.read(1)",
        "        if not c: return None",
        "        h += c",
        "    n = 0",
        r"    for l in h.decode().split('\r\n'):",
        r"        if l.lower().startswith('content-length:'): n = int(l.split(':',1)[1].strip())",
        "    return json.loads(sys.stdin.buffer.read(n).decode())",
        "def send_msg(m):",
        "    p = json.dumps(m).encode()",
        r"    sys.stdout.buffer.write(f'Content-Length: {len(p)}\r\n\r\n'.encode() + p); sys.stdout.buffer.flush()",
        "while True:",
        "    r = read_msg()",
        "    if r is None: break",
        "    m = r['method']",
        "    if m == 'initialize':",
        "        if FAIL_ONCE == 'initialize_hang' and fail_once():",
        "            while True: time.sleep(1)",
        "        init_count += 1",
        "        send_msg({'jsonrpc':'2.0','id':r['id'],'result':{'protocolVersion':r['params']['protocolVersion'],'capabilities':{'tools':{}},'serverInfo':{'name':LABEL,'version':'1.0.0'}}})",
        "    elif m == 'tools/list':",
        "        send_msg({'jsonrpc':'2.0','id':r['id'],'result':{'tools':[{'name':'echo','description':f'Echo for {LABEL}','inputSchema':{'type':'object','properties':{'text':{'type':'string'}},'required':['text']}}]}})",
        "        if EXIT_AFTER: raise SystemExit(0)",
        "    elif m == 'tools/call':",
        "        if FAIL_ONCE == 'tool_call_disconnect' and fail_once(): raise SystemExit(0)",
        "        if DELAY: time.sleep(DELAY / 1000)",
        "        t = (r['params'].get('arguments') or {}).get('text', '')",
        "        send_msg({'jsonrpc':'2.0','id':r['id'],'result':{'content':[{'type':'text','text':f'{LABEL}:{t}'}],'structuredContent':{'server':LABEL,'echoed':t,'env_test_var':TEST_VAR,'initCount':init_count},'isError':False}})",
        "    else: send_msg({'jsonrpc':'2.0','id':r['id'],'error':{'code':-32601,'message':f'unknown: {m}'}})",
        "",
    ].join("\n");
    fs::write(&script_path, script).unwrap();
    let mut perms = fs::metadata(&script_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms).unwrap();
    script_path
}

fn srv(script: &Path, label: &str, extra_env: BTreeMap<String, String>) -> ScopedMcpServerConfig {
    let mut env = BTreeMap::from([("MCP_SERVER_LABEL".into(), label.into())]);
    env.extend(extra_env);
    ScopedMcpServerConfig {
        scope: ConfigSource::Local,
        config: McpServerConfig::Stdio(McpStdioServerConfig {
            command: "python3".into(),
            args: vec![script.to_string_lossy().into_owned()],
            env,
            tool_call_timeout_ms: None,
        }),
    }
}

fn srv_timeout(
    script: &Path,
    label: &str,
    ms: u64,
    extra_env: BTreeMap<String, String>,
) -> ScopedMcpServerConfig {
    let mut env = BTreeMap::from([("MCP_SERVER_LABEL".into(), label.into())]);
    env.extend(extra_env);
    ScopedMcpServerConfig {
        scope: ConfigSource::Local,
        config: McpServerConfig::Stdio(McpStdioServerConfig {
            command: "python3".into(),
            args: vec![script.to_string_lossy().into_owned()],
            env,
            tool_call_timeout_ms: Some(ms),
        }),
    }
}

fn cleanup(p: &Path) {
    let _ = fs::remove_file(p);
    if let Some(d) = p.parent() {
        let _ = fs::remove_dir_all(d);
    }
}

fn tool_text(resp: &ninmu_runtime::JsonRpcResponse<ninmu_runtime::McpToolCallResult>) -> String {
    resp.result.as_ref().unwrap().content[0]
        .data
        .get("text")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn config_to_discovery_to_call_to_shutdown() {
    let rt = Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(async {
        let sa = write_mcp_server_script("a");
        let sb = write_mcp_server_script("b");
        let servers = BTreeMap::from([
            ("srv-a".into(), srv(&sa, "alpha", BTreeMap::new())),
            ("srv-b".into(), srv(&sb, "beta", BTreeMap::new())),
        ]);
        let mut mgr = McpServerManager::from_servers(&servers);
        let report = mgr.discover_tools_best_effort().await;
        assert!(
            report.failed_servers.is_empty(),
            "failures: {:?}",
            report.failed_servers
        );
        assert_eq!(report.tools.len(), 2);

        let r = mgr
            .call_tool("mcp__srv-a__echo", Some(json!({"text":"hi-a"})))
            .await
            .unwrap();
        assert!(
            tool_text(&r).contains("alpha:hi-a"),
            "got: {}",
            tool_text(&r)
        );

        let r = mgr
            .call_tool("mcp__srv-b__echo", Some(json!({"text":"hi-b"})))
            .await
            .unwrap();
        assert!(tool_text(&r).contains("beta:hi-b"));

        mgr.shutdown().await.unwrap();
        cleanup(&sa);
        cleanup(&sb);
    });
}

#[test]
fn server_crash_during_call_is_handled() {
    let rt = Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(async {
        let script = write_mcp_server_script("crash");
        let env = BTreeMap::from([("MCP_EXIT_AFTER_TOOLS_LIST".into(), "1".into())]);
        let servers = BTreeMap::from([("crashy".into(), srv(&script, "crash", env))]);
        let mut mgr = McpServerManager::from_servers(&servers);
        let report = mgr.discover_tools_best_effort().await;
        assert!(
            report.failed_servers.is_empty(),
            "discovery should succeed before exit"
        );

        let result = mgr
            .call_tool("mcp__crashy__echo", Some(json!({"text":"after-exit"})))
            .await;
        assert!(
            result.is_err(),
            "call after server exit should fail: {:?}",
            result
        );

        mgr.shutdown().await.unwrap();
        cleanup(&script);
    });
}

#[test]
fn config_with_bad_command_fails_gracefully() {
    let rt = Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(async {
        let cfg = ScopedMcpServerConfig {
            scope: ConfigSource::Local,
            config: McpServerConfig::Stdio(McpStdioServerConfig {
                command: "/nonexistent/binary".into(),
                args: vec![],
                env: BTreeMap::new(),
                tool_call_timeout_ms: None,
            }),
        };
        let mut mgr = McpServerManager::from_servers(&BTreeMap::from([("bad".into(), cfg)]));
        let report = mgr.discover_tools_best_effort().await;
        assert_eq!(report.failed_servers.len(), 1);
        assert_eq!(report.failed_servers[0].server_name, "bad");
        mgr.shutdown().await.unwrap();
    });
}

#[test]
fn tool_call_timeout_is_enforced() {
    let rt = Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(async {
        let script = write_mcp_server_script("slow");
        let env = BTreeMap::from([("MCP_TOOL_CALL_DELAY_MS".into(), "5000".into())]);
        let servers = BTreeMap::from([("slow".into(), srv_timeout(&script, "slow", 500, env))]);
        let mut mgr = McpServerManager::from_servers(&servers);
        assert!(mgr
            .discover_tools_best_effort()
            .await
            .failed_servers
            .is_empty());

        let res = mgr
            .call_tool("mcp__slow__echo", Some(json!({"text":"x"})))
            .await;
        assert!(res.is_err());
        match res.unwrap_err() {
            McpServerManagerError::Timeout { .. } => {}
            e => panic!("expected Timeout, got: {:?}", e),
        }
        mgr.shutdown().await.unwrap();
        cleanup(&script);
    });
}

#[test]
fn multi_server_isolated_tool_namespaces() {
    let rt = Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(async {
        let sa = write_mcp_server_script("na");
        let sb = write_mcp_server_script("nb");
        let servers = BTreeMap::from([
            ("alpha".into(), srv(&sa, "nsa", BTreeMap::new())),
            ("beta".into(), srv(&sb, "nsb", BTreeMap::new())),
        ]);
        let mut mgr = McpServerManager::from_servers(&servers);
        let report = mgr.discover_tools_best_effort().await;
        assert_eq!(report.tools.len(), 2);
        let names: Vec<&str> = report
            .tools
            .iter()
            .map(|t| t.qualified_name.as_str())
            .collect();
        assert!(names.contains(&"mcp__alpha__echo"), "{:?}", names);
        assert!(names.contains(&"mcp__beta__echo"), "{:?}", names);

        assert!(tool_text(
            &mgr.call_tool("mcp__alpha__echo", Some(json!({"text":"x"})))
                .await
                .unwrap()
        )
        .contains("nsa:x"));
        assert!(tool_text(
            &mgr.call_tool("mcp__beta__echo", Some(json!({"text":"y"})))
                .await
                .unwrap()
        )
        .contains("nsb:y"));

        mgr.shutdown().await.unwrap();
        cleanup(&sa);
        cleanup(&sb);
    });
}

#[test]
fn shutdown_is_idempotent_and_reusable() {
    let rt = Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(async {
        let script = write_mcp_server_script("idem");
        let servers = BTreeMap::from([("idem".into(), srv(&script, "idem", BTreeMap::new()))]);
        let mut mgr = McpServerManager::from_servers(&servers);
        mgr.discover_tools_best_effort().await;
        mgr.shutdown().await.unwrap();
        mgr.shutdown().await.unwrap();

        let r = mgr
            .call_tool("mcp__idem__echo", Some(json!({"text":"revived"})))
            .await;
        assert!(r.is_ok(), "should re-spawn: {:?}", r);
        assert!(tool_text(&r.unwrap()).contains("revived"));

        mgr.shutdown().await.unwrap();
        cleanup(&script);
    });
}

#[test]
fn environment_variables_propagate_to_server() {
    let rt = Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(async {
        let script = write_mcp_server_script("env");
        let env = BTreeMap::from([("TEST_VAR".into(), "hello-config".into())]);
        let servers = BTreeMap::from([("esrv".into(), srv(&script, "env", env))]);
        let mut mgr = McpServerManager::from_servers(&servers);
        mgr.discover_tools_best_effort().await;

        let r = mgr
            .call_tool("mcp__esrv__echo", Some(json!({"text":"check"})))
            .await
            .unwrap();
        let sc = r.result.unwrap().structured_content.unwrap();
        assert_eq!(
            sc.get("env_test_var").and_then(|v| v.as_str()),
            Some("hello-config"),
            "got: {:?}",
            sc
        );

        mgr.shutdown().await.unwrap();
        cleanup(&script);
    });
}

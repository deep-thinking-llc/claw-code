# Code Review — Ninmu Code

**Date**: 2026-04-28  
**Scope**: `rust/crates/`, `src/`, `tests/`, `python/`, root markdown files  
**Verified**: Second pass confirmed accuracy of all findings

---

## 1. Security Issues

### S-1: Command Injection via Shell Interpolation in `command_exists()`
- **Severity**: HIGH
- **File**: `rust/crates/ninmu-tools/src/lib.rs:5915`
- **Description**: `command_exists()` passes an unsanitized `command` string via `format!("command -v {command} >/dev/null 2>&1")` into `sh -lc`. Currently only called with hardcoded shell names (`sh`, `bash`, `zsh`, `fish`), so exploitability is limited, but the pattern is dangerous if refactored.
- **Fix**: Use `std::process::Command::new(command)` with `--version` flag, or validate the command name (alphanumeric + `-` + `_` only) before interpolation.

### S-2: OAuth Credentials Written Without Restricted Permissions
- **Severity**: MEDIUM
- **File**: `rust/crates/ninmu-runtime/src/oauth.rs:378`
- **Description**: `write_credentials_root()` writes OAuth tokens via `fs::write()` with no `chmod` call. Files are created with the process umask (typically `0644`), meaning other users on the system can read credentials.
- **Fix**: Use `std::os::unix::fs::PermissionsExt` to set mode `0o600` after writing, or use `OpenOptions` with appropriate mode.

### S-3: Webhook Auth Header Visible in Process Arguments
- **Severity**: MEDIUM
- **File**: `rust/crates/ninmu-sdk/src/notification.rs:243`
- **Description**: `WebhookSink` passes the `Authorization` header value via `cmd.arg(format!("Authorization: {auth}"))`, visible in `ps aux`.
- **Fix**: Pass the auth header via stdin or environment variable instead of CLI argument.

### S-4: `dangerouslyDisableSandbox` Flag Naming
- **Severity**: LOW
- **File**: `rust/crates/ninmu-runtime/src/bash.rs:26`
- **Description**: The `dangerously_disable_sandbox` field name is self-documenting but the flag is propagated without a visible authorization gate at the struct level. The actual enforcement happens in `sandbox_status_for_input()`.
- **Fix**: Add a doc comment noting that the flag is only respected when validated against the runtime permission policy.

### S-5: Placeholder API Key Patterns in Documentation
- **Severity**: LOW
- **Files**: `README.md`, `install.sh`, `rust/README.md`
- **Description**: Documentation uses realistic-looking placeholder patterns like `sk-ant-...` that can trigger secret scanners.
- **Fix**: Use `<your-api-key>` or `<API_KEY_HERE>` instead.

---

## 2. Dead Code

### DC-1: 5 Workspace-Boundary Functions in `file_ops.rs`
- **File**: `rust/crates/ninmu-runtime/src/file_ops.rs:32, 571, 588, 604, 622`
- **Description**: `validate_workspace_boundary()`, `read_file_in_workspace()`, `write_file_in_workspace()`, `edit_file_in_workspace()`, `is_symlink_escape()` — all marked `#[allow(dead_code)]`. Planned for future path-sandboxing.
- **Fix**: Acceptable with existing TODO comments. Consider gating behind a feature flag.

### DC-2: `workspace_sessions_dir()` in `session.rs`
- **File**: `rust/crates/ninmu-runtime/src/session.rs:1496`
- **Description**: Public function with no internal callers. Intended for external consumers.
- **Fix**: Add doc comment clarifying external API purpose.

### DC-3: Legacy `src/` Python Directory
- **Files**: `src/*.py` (~60 files)
- **Description**: Original Python porting workspace, not part of the active Rust product.
- **Fix**: Add a `src/README.md` explaining purpose, or move to `archive/`.

---

## 3. Stale / Inaccurate Documentation

### SD-1: `HANDOFF.md` Outdated
- **File**: `HANDOFF.md`
- **Description**: References specific commit hashes and says "ahead of origin by 2 commits" — both stale.
- **Fix**: Update or remove if the handoff period is complete.

### SD-2: `TEST_COVERAGE_REPORT.md` Documents Pre-existing Failures
- **File**: `TEST_COVERAGE_REPORT.md`
- **Description**: Correctly documents 3 pre-existing test failures. The Ollama/vLLM routing failures are due to incomplete env var cleanup in tests, not a logic bug in production code.
- **Fix**: Update once the test isolation is improved.

### SD-3: `CLAUDE.md` (root) Ambiguous About `src/`
- **File**: `CLAUDE.md` (root)
- **Description**: Says "`src/` and `tests/` are both present; update both surfaces together." This implies `src/` Python code is active, which could confuse AI agents.
- **Fix**: Clarify that `src/` is a legacy reference and `rust/` is the active codebase.

### SD-4: README SDK Example Uses Wrong Crate Names and Paths
- **File**: `README.md` (SDK section)
- **Description**: Shows `sdk = { path = "../ninmu-code/rust/crates/sdk" }` — wrong path and crate name.
- **Fix**: Update to `ninmu-sdk = { path = "../rust/crates/ninmu-sdk" }`.

### SD-5: README Provider Table Missing Mistral, Gemini, Cohere
- **File**: `README.md` (Built-in Providers table)
- **Description**: Lists 8 providers but 11 are implemented (Mistral, Gemini, Cohere are missing).
- **Fix**: Add the three missing providers.

---

## 4. Bugs and Inconsistencies

### B-1: Incomplete Env Var Cleanup in Provider Detection Tests
- **Severity**: MEDIUM
- **File**: `rust/crates/ninmu-api/src/providers/mod.rs` (tests)
- **Description**: Tests for `detect_provider_from_ollama_base_url` and `detect_provider_from_vllm_base_url` fail when `ANTHROPIC_AUTH_TOKEN` or `OPENAI_BASE_URL` are set in the environment, because those are checked before OLLAMA_BASE_URL/VLLM_BASE_URL. The production code order is intentional (API-key providers first, then base-URL-only providers), but the tests don't fully isolate the environment.
- **Fix**: Add `env_lock()` + `remove_var` for all provider env vars in these tests.

### B-2: Inconsistent Environment Variable Naming
- **Severity**: MEDIUM
- **Files**: Multiple across `rust/crates/`
- **Description**: Uses `CLAUDE_CODE_REMOTE`, `CLAUDE_CODE_UPSTREAM`, `CLAUDE_CONFIG_HOME`, `CLAWD_SANDBOX_FILESYSTEM_MODE` — mix of upstream fork naming and product naming.
- **Fix**: Adopt a canonical prefix and add backward-compatible aliases.

### B-3: `iso8601_timestamp()` Shells Out to External `date` Command
- **Severity**: LOW
- **File**: `rust/crates/ninmu-tools/src/lib.rs:5876`
- **Description**: Spawns an external `date` process for something Rust can do natively. Has a fallback to `iso8601_now()` but the primary path is unnecessary.
- **Fix**: Use `iso8601_now()` directly, remove the `Command::new("date")` call.

### B-4: 4 Duplicate `command_exists()` Implementations
- **Severity**: LOW
- **Files**: `ninmu-tools/src/lib.rs:5915`, `ninmu-runtime/src/sandbox.rs:280`, `ninmu-cli/src/main.rs:1944`, `ninmu-cli/src/cli_commands.rs:1656`
- **Description**: Four separate implementations with different strategies (shell interpolation, PATH walk, `which`). The sandbox one is safest.
- **Fix**: Extract a shared `command_exists()` utility into `ninmu-tools` or a shared crate.

---

## 5. Dependency Issues

### DI-1: `ninmu-sdk` `rpc` Feature Is Default-Enabled
- **File**: `rust/crates/ninmu-sdk/Cargo.toml`
- **Description**: The `rpc` feature is in `default` and gates the `rpc` module. It's always enabled unless explicitly disabled.
- **Fix**: Consider removing from defaults if RPC is optional, or removing the feature gate entirely if it's always needed.

---

## Summary

| Category | HIGH | MEDIUM | LOW |
|----------|------|--------|-----|
| Security | 1 | 2 | 2 |
| Dead Code | 0 | 1 | 2 |
| Stale Docs | 0 | 2 | 3 |
| Bugs | 0 | 2 | 2 |
| Dependencies | 0 | 0 | 1 |
| **Total** | **1** | **7** | **10** |

### Top Priority Fixes
1. **S-1**: Sanitize `command_exists()` in ninmu-tools
2. **S-2**: Set `0o600` on OAuth credentials file
3. **B-1**: Fix env var cleanup in provider detection tests
4. **SD-4/SD-5**: Fix README SDK example and provider table

# Security Implementation Plan — Comprehensive Fix

**Scope**: All findings from the 2026-04-29 security review of the `ninmu-code` repository.
**Files affected**: `oauth.rs`, `shared_staging.rs`, `message_bus.rs`, `policy_engine.rs`, `security.rs`, `client.py`, `install.js`, `download.sh`, `deploy/Dockerfile`, `runtime/policy_engine.rs`
**Expected outcome**: Zero P0/P1 findings, all P2 findings resolved, P3 findings resolved where feasible.
**Test target**: Every fix has at least one regression test.
**Plan version**: 1.1 (patched after second-pass review)

---

## Table of Contents

1. [P0 — Critical](#p0-critical)
2. [P1 — High](#p1-high)
3. [P2 — Medium](#p2-medium)
4. [P3 — Low](#p3-low)
5. [Cross-Cutting Concerns](#cross-cutting-concerns)
6. [Test Matrix](#test-matrix)
7. [Implementation Order](#implementation-order)

---

## P0 — Critical

### P0.1 OAuth tokens stored in plaintext on disk

**Location**: `crates/ninmu-runtime/src/oauth.rs:283-294` (`save_oauth_credentials`), `credentials_home_dir()`, `write_credentials_root()`

**Current behavior**:
- Access tokens, refresh tokens, and scopes are serialized to `~/.ninmu/credentials.json` in plain text.
- No encryption, no keychain integration, no restrictive file permissions.
- On shared/multi-user systems, other users can read the file (default umask `0o644`).

**Exploit path**: A malicious process or user with filesystem access reads `~/.ninmu/credentials.json` and obtains the `access_token`, enabling full API access.

**Fix**:

1. **Immediate**: Restrict file permissions after write.
   - After `write_credentials_root()`, on Unix:
     ```rust
     #[cfg(unix)]
     {
         use std::os::unix::fs::PermissionsExt;
         fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
     }
     ```
   - After `write_credentials_root()`, on the parent `.ninmu` directory:
     ```rust
     #[cfg(unix)]
     {
         if let Some(parent) = path.parent() {
             fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
         }
     }
     ```

2. **Medium-term**: Integrate with OS keychain.
   - Add `keyring` crate (cross-platform, uses macOS Keychain, Windows Credential Manager, Linux Secret Service).
   - `save_oauth_credentials` tries keyring first, falls back to encrypted file if keyring unavailable.
   - Encrypted file: use `tokio_rustls` or `ring` to derive an encryption key from the user's machine ID.
   - For now, just do chmod; add a `TODO(keyring)` comment.

**Tests**:
- `test_credentials_file_mode_is_restricted`: write a token, stat the file, assert mode `0o600` on Unix.
- `test_credentials_dir_mode_is_restricted`: assert `~/.ninmu` is `0o700`.
- `test_credentials_round_trip_preserved`: existing test still passes (regression).

---

## P1 — High

### P1.1 OAuth `state` parameter generated but never validated on callback

**Location**: `crates/ninmu-runtime/src/oauth.rs:301-308` (`parse_oauth_callback_request_target`), missing state validation in caller code.

**Current behavior**:
- `generate_state()` creates a 32-byte random token.
- `OAuthAuthorizationRequest` includes `state` in the authorize URL.
- `OAuthCallbackParams` parses the returned state from the callback.
- **No caller in the codebase compares callback.state with the generated state.**

**Exploit path**: An attacker redirects the user to `/callback?code=STOLEN_CODE&state=ANYTHING`. The CLI exchanges the stolen code for tokens belonging to the attacker or a confused user.

**Fix**: 
1. In `build_authorize_url`, `state` is passed through `generate_state()` as a single-use token. The `OAuthCallbackParams` carries this back.
2. In `parse_oauth_callback_request_target`, enforce that `params.state` MUST match the stored state for the in-flight flow.
3. Store pending states in an in-memory `HashMap` keyed by `state` value, mapping to the `PkceCodePair` and `redirect_uri` used for the request.
4. In `parse_oauth_callback_request_target`, reject the callback if `params.state` is `None`, or if it does not match a key in the pending state map.
5. Remove the consumed state from the map immediately after validation to prevent replay.

**Implementation sketch**:

```rust
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct PendingOAuthFlow {
    pub state: String,
    pub code_verifier: String,
    pub redirect_uri: String,
    pub created_at: Instant,
}

#[derive(Debug)]
pub struct OAuthFlowStore {
    pub flows: Arc<Mutex<HashMap<String, PendingOAuthFlow>>>,
    pub max_age: Duration,
}

impl OAuthFlowStore {
    pub fn new(max_age: Duration) -> Self {
        Self { flows: Arc::new(Mutex::new(HashMap::new())), max_age }
    }
    
    pub fn register(&self, flow: PendingOAuthFlow) {
        let mut flows = self.flows.lock().expect("lock");
        flows.insert(flow.state.clone(), flow);
    }
    
    pub fn validate_and_remove(&self, state: &str) -> Result<PendingOAuthFlow, String> {
        let mut flows = self.flows.lock().expect("lock");
        let flow = flows.remove(state).ok_or_else(|| "state not found or already consumed".to_string())?;
        if flow.created_at.elapsed() > self.max_age {
            return Err("state expired".to_string());
        }
        Ok(flow)
    }
}
```

On the caller side (browser flow initiator):
```rust
let store = OAuthFlowStore::new(Duration::from_secs(600));
let state = generate_state()?;
let pkce = generate_pkce_pair()?;
let redirect_uri = loopback_redirect_uri(4545);
let req = OAuthAuthorizationRequest::from_config(&config, &redirect_uri, &state, &pkce);
store.register(PendingOAuthFlow { state: state.clone(), code_verifier: pkce.verifier.clone(), redirect_uri, created_at: Instant::now() });
// open browser, start listener
// ...
let callback_params = parse_oauth_callback_request_target(target, &store)?;
```

`parse_oauth_callback_request_target` updated:
```rust
pub fn parse_oauth_callback_request_target(
    target: &str,
    store: &OAuthFlowStore,
) -> Result<OAuthCallbackParams, String> {
    let (path, query) = target.split_once('?').map_or((target, ""), |(p, q)| (p, q));
    if path != "/callback" {
        return Err(format!("unexpected callback path: {path}"));
    }
    let params = parse_oauth_callback_query(query)?;
    let state = params.state.as_ref().ok_or("missing state parameter")?;
    let _flow = store.validate_and_remove(state)?;
    Ok(params)
}
```

**Tests**:
- `test_valid_state_exchanges`: generate state, store it, parse callback with same state, succeed.
- `test_mismatched_state_rejected`: parse callback with wrong state, fails.
- `test_missing_state_rejected`: callback without state, fails.
- `test_expired_state_rejected`: create flow, wait > max_age, exchange fails.
- `test_replay_state_rejected`: exchange same state twice, second fails.

---

### P1.2 Missing `redirect_uri` validation in OAuth callback

**Location**: `crates/ninmu-runtime/src/oauth.rs:301-308` (`parse_oauth_callback_request_target`)

**Current behavior**: Only checks `path == "/callback"`. The port is not validated.

**Exploit path**: Attacker binds `http://localhost:5555/callback` and tricks the user into authorization with `redirect_uri=localhost:5555`. The callback handler accepts it.

**Fix**: Store the exact `redirect_uri` in the `OAuthFlowStore` (from P1.1) and compare it with the callback target's origin. The `parse_oauth_callback_request_target` should also extract and validate the host:port.

```rust
pub fn parse_oauth_callback_request_target(
    target: &str,
    expected_redirect_uri: &str,
) -> Result<OAuthCallbackParams, String> {
    let (path, query) = target
        .split_once('?')
        .map_or((target, ""), |(p, q)| (p, q));
    if path != "/callback" {
        return Err(format!("unexpected callback path: {path}"));
    }
    // Validate redirect_uri prefix matches
    let expected = expected_redirect_uri
        .split_once("/callback")
        .map_or(expected_redirect_uri, |(base, _)| base);
    if !target.starts_with(expected) && target != expected_redirect_uri {
        return Err("redirect_uri mismatch".to_string());
    }
    parse_oauth_callback_query(query)
}
```

**Tests**:
- `test_redirect_uri_match_succeeds`: callback from expected URI succeeds.
- `test_redirect_uri_mismatch_fails`: callback from different port fails.

---

### P1.3 `shared_staging.rs` path traversal via `task_id`

**Location**: `crates/ninmu-sdk/src/shared_staging.rs:49-50` (`resolve_path`)

**Current behavior**: `validate_path` only checks `rel_path`, not `task_id`. If `task_id = "../../etc"` and `rel_path = "passwd"`, the resolved path is `{root}/../../etc/passwd`.

**Exploit path**: Malicious agent crafts `task_id` with `..` components to read/write arbitrary files.

**Fix**:
1. Reuse `validate_path` on `task_id`:
   ```rust
   Self::validate_path(task_id)?;
   Self::validate_path(rel_path)?;
   ```
2. **Defense in depth**: After resolving, canonicalize and ensure prefix:
   ```rust
   fn resolve_and_validate(&self, task_id: &str, rel_path: &str) -> Result<PathBuf, String> {
       let path = self.resolve_path(task_id, rel_path);
       let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
       let root_canonical = self.root.canonicalize().unwrap_or_else(|_| self.root.clone());
       if !canonical.starts_with(&root_canonical) {
           return Err("resolved path escapes staging root".to_string());
       }
       Ok(path)
   }
   ```
   Note: `canonicalize` requires the path to exist, so create parent dirs first, then validate.

3. Sanitize `task_id`: restrict to `^[a-zA-Z0-9_-]+$` or use `validate_path` which rejects `..` and absolute paths.

**Tests**:
- `test_task_id_traversal_blocked`: task_id `"../../etc"`, rel_path `"passwd"` fails.
- `test_task_id_absolute_blocked`: task_id `"/etc"`, rel_path `"passwd"` fails.
- `test_task_id_normal_succeeds`: task_id `"task-1"`, rel_path `"file.rs"` succeeds.
- `test_canonicalize_defense`: create a symlink attack, ensure it fails.

---

### P1.4 `shared_staging.rs` non-atomic file writes

**Location**: `crates/ninmu-sdk/src/shared_staging.rs:100-102` (`write`), `189-195` (`promote`)

**Current behavior**: Direct `std::fs::write` and `std::fs::copy`. Crash mid-write leaves half-written files.

**Fix**:
1. Add atomic write helper:
   ```rust
   fn atomic_write(path: &Path, content: &[u8]) -> Result<(), String> {
       let tmp = path.with_extension("tmp");
       std::fs::write(&tmp, content)
           .map_err(|e| format!("write tmp failed: {e}"))?;
       std::fs::rename(&tmp, path)
           .map_err(|e| format!("rename failed: {e}"))?;
       Ok(())
   }
   ```
2. Use in `write`:
   ```rust
   atomic_write(&path, content.as_bytes())?;
   ```
3. Use in `promote` (copy + rename):
   ```rust
   let tmp = dst.with_extension("tmp");
   std::fs::copy(&src, &tmp).map_err(...)?;
   std::fs::rename(&tmp, &dst).map_err(...)?;
   ```

**Tests**:
- `test_atomic_write_succeeds`: write, verify file exists, verify no `.tmp` left.
- `test_atomic_write_rollbacks_on_failure`: simulate failure, verify old file intact.

---

### P1.5 `MessageBus` does not authenticate message sender

**Location**: `crates/ninmu-sdk/src/message_bus.rs:24-56` (`AgentMessage`, `publish`)

**Current behavior**: `from_agent` is a free-form string. Any publisher can impersonate any agent.

**Exploit path**: Malicious agent publishes `AgentMessage { from_agent: "admin", channel: "deploy", payload: { "action": "prod_deploy" } }`. Downstream consumers trust it.

**Fix**:
1. Add `PublisherToken` tied to a verified identity:
   ```rust
   #[derive(Debug, Clone)]
   pub struct PublisherToken {
       agent_id: String,
       // HMAC or signature of message body
       signature: Option<String>,
   }
   ```
2. The `MessageBus` holds a `HashMap<String, PublisherToken>` mapping agent_id to registered tokens.
3. `publish` requires a `PublisherToken` (or `Arc<PublisherToken>`):
   ```rust
   pub fn publish(&self, token: &PublisherToken, topic: &str, message: AgentMessage) {
       if message.from_agent != token.agent_id {
           return; // or log + drop
       }
       // proceed...
   }
   ```
4. Add bus-level registration:
   ```rust
   pub fn register_agent(&self, agent_id: &str) -> PublisherToken { ... }
   ```

**Tests**:
- `test_unregistered_agent_cannot_publish`: publish without token, message dropped.
- `test_wrong_agent_id_rejected`: token for "alice", message says "bob", dropped.
- `test_registered_agent_can_publish": succeeds.

---

### P1.6 `AuditLog::file` accepts arbitrary path injection

**Location**: `crates/ninmu-sdk/src/security.rs:255-280` (`AuditLog::file`)

**Current behavior**: `path` is any `PathBuf`. `fs::create_dir_all(parent)` creates arbitrary directories.

**Exploit path**: If `path` derived from untrusted input (e.g., session name from remote), attacker creates directories anywhere writable.

**Fix**:
1. Validate path under a known base:
   ```rust
   pub fn file(path: impl Into<PathBuf>) -> Result<Self, String> {
       let path = path.into();
       if let Some(parent) = path.parent() {
           let canonical = parent.canonicalize().unwrap_or_else(|_| parent.to_path_buf());
           let base = AUDIT_BASE.canonicalize().unwrap_or_else(|_| AUDIT_BASE.to_path_buf());
           if !canonical.starts_with(&base) {
               return Err("audit path escapes base directory".to_string());
           }
           fs::create_dir_all(parent)?;
       }
       // ...
   }
   ```
2. Or simpler: accept only a filename, prepend a fixed base:
   ```rust
   pub fn file(filename: &str) -> Result<Self, String> {
       let path = AUDIT_BASE.join(filename);
       // validate filename has no path separators
       if filename.contains('/') || filename.contains('\\') {
           return Err("invalid filename".to_string());
       }
       // ...
   }
   ```

**Tests**:
- `test_audit_path_traversal_blocked`: filename `"../../etc/passwd"`, fails.
- `test_audit_path_normal_succeeds`: filename `"audit-2024.jsonl"`, succeeds.

---

### P1.7 Python `NinmuClient` passes untrusted binary name to subprocess

**Location**: `python/ninmu_py/ninmu/client.py:54-67` (`__init__`)

**Current behavior**: `binary` parameter is passed directly to `subprocess.Popen([binary, "rpc"], ...)`.

**Exploit path**: `NinmuClient(binary="../../malicious")` executes arbitrary code.

**Fix**:
1. Validate binary resolves to known path:
   ```python
   import shutil
   import os

   def _resolve_binary(binary: str) -> str:
       if os.path.isabs(binary):
           if not os.path.isfile(binary):
               raise NinmuBinaryError(f"binary not found: {binary!r}")
           # Optional: verify it's actually the ninmu binary
           return binary
       resolved = shutil.which(binary)
       if resolved is None:
           raise NinmuBinaryError(f"binary not found on PATH: {binary!r}")
       return resolved
   ```
2. Reject paths with `..` components:
   ```python
   if ".." in binary or binary.startswith("/") and not os.path.isfile(binary):
       raise NinmuBinaryError(...)
   ```

**Tests**:
- `test_binary_path_traversal_rejected`: `binary="../malicious"` raises `NinmuBinaryError`.
- `test_binary_relative_not_on_path_rejected`: `binary="./ninmu"` not in PATH, rejected.
- `test_binary_absolute_missing_rejected`: `binary="/nonexistent/ninmu"` rejected.
- `test_binary_shutil_which_succeeds`: valid binary found via `shutil.which`, succeeds.

---

### P1.8 Percent-encode ambiguity in OAuth query string

**Location**: `crates/ninmu-runtime/src/oauth.rs:376-409` (`percent_encode`, `percent_decode`)

**Current behavior**: Custom percent-encode/decode. `percent_decode` treats `+` as space. `percent_encode` encodes literal `+` as `%2B`.

**Exploit path**: If a scope or state contains `+`, the authorization server may receive `%2B` but decode it as ` ` depending on their decoder. This can cause scope mismatch or state mismatch.

**Fix**: Replace custom percent-encode/decode with the `percent-encoding` crate (already widely used, no new heavy deps).

```rust
use percent_encoding::{percent_encode, percent_decode_str, NON_ALPHANUMERIC};

fn percent_encode(value: &str) -> String {
    percent_encode(value.as_bytes(), NON_ALPHANUMERIC).to_string()
}

fn percent_decode(value: &str) -> Result<String, String> {
    percent_decode_str(value)
        .decode_utf8()
        .map_err(|e| format!("invalid utf8: {e}"))
        .map(|s| s.to_string())
}
```

Note: The `percent-encoding` crate handles `+` correctly (does NOT treat it as space, per RFC 3986). If the server expects `application/x-www-form-urlencoded` decoding, we should use `+` for spaces. However, the `state` and `code` parameters shouldn't contain spaces, and `scope` uses space-separated values in the query string.

Actually, for `application/x-www-form-urlencoded` (which OAuth uses), `+` should be space. Let me revise:

```rust
use percent_encoding::{percent_encode, percent_decode_str, NON_ALPHANUMERIC};

fn percent_encode(value: &str) -> String {
    percent_encode(value.as_bytes(), NON_ALPHANUMERIC).to_string()
}

fn percent_decode(value: &str) -> Result<String, String> {
    // First replace + with space (form-urlencoded), then percent-decode
    let normalized = value.replace('+', " ");
    percent_decode_str(&normalized)
        .decode_utf8()
        .map_err(|e| format!("invalid utf8: {e}"))
        .map(|s| s.to_string())
}
```

**Tests**:
- `test_percent_encode_decode_roundtrip`: `"hello world+foo"` encodes then decodes correctly.
- `test_percent_decode_plus_is_space`: `"hello+world"` decodes to `"hello world"`.
- `test_percent_decode_invalid_utf8_fails`: `"%FF%FE"` fails gracefully.

---

### P1.9 `SecretScrubber` short secret partial leak

**Location**: `crates/ninmu-sdk/src/security.rs:136-148` (`scrub_env`)

**Current behavior**: If secret value length <= 6, fully redacted. If > 6, first 4 chars visible.

**Exploit path**: A 6-char API key `"abc123"` becomes `"abc1[REDACTED]"` — leaking 4 chars of a very short secret is a non-trivial reduction in brute-force space.

**Fix**: Use proportional preview or always redact short secrets:
```rust
let visible = if v.len() <= 8 {
    0
} else {
    (v.len() / 4).min(4)
};
let preview = v.chars().take(visible).collect::<String>();
(k, format!("{preview}[REDACTED]"))
```

**Tests**:
- `test_short_secret_fully_redacted`: 6-char secret shows `[REDACTED]` with no prefix.
- `test_long_secret_partial_preview`: 40-char secret shows 4-char prefix + `[REDACTED]`.

---

## P2 — Medium

### P2.1 `StagingLock` never auto-expires

**Location**: `crates/ninmu-sdk/src/shared_staging.rs:12-17` (`StagingLock`), `170-178` (`unlock`)

**Current behavior**: Locks expire only when `lock()` checks `at.elapsed() > tmo`. An abandoned lock never gets cleaned up.

**Fix**: Add `StagingLockGuard` implementing `Drop`:
```rust
pub struct StagingLockGuard<'a> {
    lock: StagingLock,
    staging: &'a SharedStaging,
}

impl<'a> Drop for StagingLockGuard<'a> {
    fn drop(&mut self) {
        self.staging.unlock(&self.lock);
    }
}

impl SharedStaging {
    pub fn lock_guard<'a>(&'a self, ...) -> Result<StagingLockGuard<'a>, String> { ... }
}
```

Also add a periodic sweep (or do it lazily on `list()` / `write()`):
```rust
fn sweep_expired(&self) {
    let mut state = self.state.lock().expect("state lock");
    let now = Instant::now();
    state.file_locks.retain(|_, (_, _, at, tmo)| now.duration_since(*at) <= *tmo);
}
```

**Tests**:
- `test_lock_guard_auto_releases`: acquire guard, drop it, verify second lock succeeds.
- `test_sweep_removes_expired`: old lock, sweep, verify removed.

---

### P2.2 `StagingLockGuard` RAII wrapper missing

**Location**: `crates/ninmu-sdk/src/shared_staging.rs` — entire file

**Current behavior**: No RAII guard exists; callers must manually call `unlock()`.

**Fix**: Same as P2.1 — implement `StagingLockGuard`.

**Tests**: Same as P2.1.

---

### P2.3 `lock_timeout` test uses hardcoded `/tmp/s`

**Location**: `crates/ninmu-sdk/src/shared_staging.rs:222-228` (test)

**Current behavior**: `SharedStaging::new(PathBuf::from("/tmp/s"))` — not isolated.

**Fix**: Use `tempfile::tempdir()`:
```rust
let dir = tempfile::tempdir().unwrap();
let s = SharedStaging::new(dir.path().join("staging"))
    .with_lock_timeout(Duration::from_millis(1));
```

**Tests**: Existing test updated.

---

### P2.4 `ConflictDetector::now_ms()` wraps on clock anomalies

**Location**: `crates/ninmu-sdk/src/conflict.rs:35-40`

**Current behavior**: `unwrap_or_default()` returns 0 if clock is before epoch.

**Fix**: Propagate error or panic (this should never happen):
```rust
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before epoch")
        .as_millis() as u64
}
```
Or return `Result<u64, String>` and bubble up.

**Tests**:
- `test_now_ms_returns_reasonable_value`: assert > 1_700_000_000_000 (after 2023).

---

### P2.5 `MergeEngine::auto_merge` incorrect deletion handling

**Location**: `crates/ninmu-sdk/src/conflict.rs:145-199`

**Current behavior**: When `their_lines` extend beyond `our_lines`, extra lines are appended. But if `ours` deleted lines from `base`, the merge may re-introduce them.

**Fix**: Use a proper diff3 or diff-match-patch algorithm. For now, add a conservative check — if file lengths differ significantly from base, return conflict:
```rust
let our_len_diff = our_lines.len() as i64 - base_lines.len() as i64;
let their_len_diff = their_lines.len() as i64 - base_lines.len() as i64;
if our_len_diff.signum() != their_len_diff.signum() && our_len_diff.abs() > 1 {
    return Err(MergeConflict { ... });
}
```

**Better fix**: Use the `similar` crate for diff algorithm.

**Tests**:
- `test_merge_with_deletions_returns_conflict`: base has 5 lines, ours deletes 2, theirs adds 1 → conflict.

---

### P2.6 `MessageBus::history` retains sensitive messages without scrubbing

**Location**: `crates/ninmu-sdk/src/message_bus.rs:36-48` (`publish`)

**Current behavior**: Messages stored in ring buffer until evicted. No scrubbing of secrets.

**Fix**: Add optional `SecretScrubber` to `MessageBus`:
```rust
pub struct MessageBus {
    // ... existing fields ...
    scrubber: Option<SecretScrubber>,
}

impl MessageBus {
    pub fn with_scrubber(mut self, scrubber: SecretScrubber) -> Self {
        self.scrubber = Some(scrubber);
        self
    }

    pub fn publish(&self, topic: &str, mut message: AgentMessage) {
        if let Some(scrubber) = &self.scrubber {
            if let Some(payload_str) = message.payload.as_str() {
                let (scrubbed, _) = scrubber.scrub(payload_str);
                message.payload = Value::String(scrubbed);
            }
        }
        // ...
    }
}
```

**Tests**:
- `test_history_scrubs_secrets`: publish message with API key, verify history has `[REDACTED]`.

---

### P2.7 `OAuthRefreshRequest` re-widens scopes

**Location**: `crates/ninmu-runtime/src/oauth.rs:223-242`

**Current behavior**: `scopes: scopes.unwrap_or_else(|| config.scopes.clone())` — on refresh, uses original authorization scopes.

**Fix**: Use the stored token's scopes. The `OAuthTokenSet` already has `scopes`, so `OAuthRefreshRequest::from_token_set`:
```rust
pub fn from_token_set(config: &OAuthConfig, token_set: &OAuthTokenSet) -> Self {
    Self {
        grant_type: "refresh_token",
        refresh_token: token_set.refresh_token.clone().unwrap_or_default(),
        client_id: config.client_id.clone(),
        scopes: token_set.scopes.clone(),
    }
}
```

**Tests**:
- `test_refresh_uses_token_scopes`: token has narrow scopes, refresh request matches them.

---

### P2.8 `/dev/urandom` fails on Windows

**Location**: `crates/ninmu-runtime/src/oauth.rs:328-334`

**Current behavior**: `File::open("/dev/urandom")` fails on Windows.

**Fix**: Use `getrandom` crate (already in most Rust crypto stacks) or `rand::thread_rng()`:
```rust
fn generate_random_token(bytes: usize) -> io::Result<String> {
    let mut buffer = vec![0_u8; bytes];
    getrandom::getrandom(&mut buffer)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
    Ok(base64url_encode(&buffer))
}
```

Add to `Cargo.toml`:
```toml
getrandom = "0.2"
```

**Tests**:
- `test_generate_random_token_succeeds`: token is non-empty, different each call.

---

### P2.9 OAuth credentials file permissions not restricted

**Location**: `crates/ninmu-runtime/src/oauth.rs:349-360` (`write_credentials_root`)

**Current behavior**: No `set_permissions` call.

**Fix**: Same as P0.1 — add `chmod 600`.

**Tests**: Same as P0.1.

---

### P2.10 `PolicyEngine` `RiskScore` is a placeholder

**Location**: `crates/ninmu-sdk/src/policy_engine.rs:187-191`

**Current behavior**: Always returns `false`.

**Fix**: Either implement risk scoring or remove the condition type.

**Tests**: Remove or implement — test for whichever chosen.

---

### P2.11 `PolicyAction` default `kind` is Execution

**Location**: `crates/ninmu-sdk/src/policy_engine.rs:56-58`

**Current behavior**: `impl Default for PolicyKind` returns `Execution`.

**Fix**: Make `kind` required — remove `Default` from `PolicyAction`, add constructor:
```rust
impl PolicyAction {
    pub fn new(kind: PolicyKind) -> Self {
        Self {
            kind,
            tool_name: None,
            // ... rest default
        }
    }
}
```

**Tests**:
- `test_policy_action_requires_kind`: compile-time check.

---

### P2.12 Python adapter import errors silently swallowed

**Location**: `python/ninmu_py/ninmu/adapters/*.py`

**Current behavior**: `try/except ImportError: pass` around framework imports.

**Fix**: Re-raise with helpful message:
```python
try:
    from langchain.tools import BaseTool
except ImportError as e:
    raise ImportError(
        "langchain is required for LangChainNinmuTool. "
        "Install it: pip install langchain"
    ) from e
```

**Tests**:
- `test_adapter_import_error_is_explicit`: monkeypatch import to fail, verify message.

---

## P3 — Low

### P3.1-8: Various minor issues

These are stylistic / edge case issues that don't have concrete exploit paths but should be cleaned up for correctness:

- **P3.1**: `base64url_encode` custom implementation — consider `base64` crate with `URL_SAFE_NO_PAD`.
- **P3.2**: `percent_decode` error message `"invalid percent byte: {byte}"` — byte is a raw u8, formatting may show non-printable.
- **P3.3**: `OAuthCallbackParams` `error_description` may contain user-controlled data — should be scrubbed before logging.
- **P3.4**: `StagingLock` `version` wraps at `u64::MAX` — practically impossible but should document.
- **P3.5**: `MessageBus` `subscribe_all` creates `__wildcard__` entry that is never cleaned up — memory leak over long runtime.
- **P3.6**: `SharedStaging::list` traverses directories without limiting depth — symlink loops could hang.
- **P3.7**: `ConflictDetector` `versions` `HashMap` growth unbounded — add LRU or max_entries.
- **P3.8**: `PolicyEngine::evaluate` sorts rules on every call — cache sorted rules.

**Fixes**:
- P3.1: Replace custom base64 with `base64` crate.
- P3.2: Format byte as hex: `"invalid percent byte: 0x{byte:02X}"`.
- P3.3: Scrub `error_description` with `SecretScrubber` before logging.
- P3.4: Document with `// u64::MAX is effectively infinite for our use case`.
- P3.5: Add `cleanup_empty_channels()` method, call periodically.
- P3.6: Add `max_depth` parameter to `list`, default 10.
- P3.7: Add `max_versions: usize` to `ConflictDetector`, evict oldest.
- P3.8: Sort once in `PolicyEngine::new`, store sorted `Vec`.

---

### P3.9 `download.sh` uses `curl | sh` without signature verification

**Location**: `scripts/download.sh`

**Current behavior**: Downloads binary via HTTPS but only checks `ninmu --version`. No cryptographic signature or checksum verification. `$SKIP_VERIFY` bypasses even that.

**Fix**: 
1. In CI release workflow, generate SHA256 checksums for each artifact and include them as release assets (`checksums.txt`).
2. In `download.sh`, after downloading, fetch `checksums.txt` and verify: `sha256sum -c checksums.txt`.
3. Fall back to `ninmu --version` only if checksums unavailable.

**Tests**: Verify script fails when checksum mismatches (dry-run test).

### P3.10 `contrib/npm/install.js` does not verify download integrity

**Location**: `contrib/npm/install.js`

**Current behavior**: Downloads release binary via HTTPS but does not verify checksums or signatures.

**Fix**: Same as P3.9 — fetch `checksums.txt` from release, verify SHA256 of downloaded binary before chmod + install.

### P3.11 `deploy/Dockerfile` runs `builder` stage as root

**Location**: `deploy/Dockerfile`

**Current behavior**: First stage builds as root. While it is a multi-stage build and the final image uses `USER ninmu`, the `builder` stage image could leak build-time secrets if pushed.

**Fix**: Add `USER ninmu: ninmu` in the builder stage too, or add a note in `README.md` that the builder image should never be pushed. Also restrict `.dockerignore` to avoid copying credentials.

### P3.12 `percent_decode` silently swallows malformed sequences

**Location**: `crates/ninmu-runtime/src/oauth.rs:311-349`

**Current behavior**: `percent_decode` treats `%` followed by non-hex bytes as literal `%` -- this could corrupt data.

**Fix**: The `percent-encoding` crate replacement (P1.8) handles this correctly -- it returns an error for invalid sequences.

**Tests covered by P1.8 tests.**

---

## Cross-Cutting Concerns

### Dependency Additions

| Crate | Purpose | Where |
|---|---|---|
| `getrandom` | Cross-platform random (replaces /dev/urandom) | `ninmu-runtime` |
| `percent-encoding` | RFC 3986 percent encode/decode | `ninmu-runtime` |
| `base64` | URL-safe base64 (replaces custom impl) | `ninmu-runtime` |
| `keyring` | OS keychain storage (future) | `ninmu-runtime` |
| `similar` | Diff algorithm for merge engine | `ninmu-sdk` |

### API Changes

- `OAuthAuthorizationRequest::from_config` signature changes to accept `OAuthFlowStore`.
- `SharedStaging::lock` returns `StagingLockGuard` (or add `lock_guard` method alongside existing `lock`).
- `MessageBus::publish` now requires `PublisherToken` — breaking change, but this module is new.
- `AuditLog::file` now accepts a base directory at construction time.

### Backward Compatibility

- OAuth changes are internal to the runtime; external API surface unchanged.
- SharedStaging changes are additive (`lock_guard` alongside `lock`).
- PolicyEngine changes are internal.
- Python client changes are additive (validation before `Popen`).

---

## Test Matrix

| Finding | Unit Test | Integration Test | Security Test |
|---|---|---|---|
| P0.1 | yes | yes | yes (file mode) |
| P1.1 | yes | — | yes (CSRF replay) |
| P1.2 | yes | — | yes (redirect uri) |
| P1.3 | yes | — | yes (path traversal) |
| P1.4 | yes | — | — |
| P1.5 | yes | — | yes (impersonation) |
| P1.6 | yes | — | yes (path injection) |
| P1.7 | yes | — | yes (subprocess injection) |
| P1.8 | yes | — | — |
| P1.9 | yes | — | — |
| P2.1 | yes | — | — |
| P2.2 | yes | — | — |
| P2.3 | yes | — | — |
| P2.4 | yes | — | — |
| P2.5 | yes | — | — |
| P2.6 | yes | — | — |
| P2.7 | yes | — | — |
| P2.8 | yes | — | — |
| P2.9 | yes | yes | yes (file mode) |
| P2.10 | yes | — | — |
| P2.11 | compile-time | — | — |
| P2.12 | yes | — | — |

**Total new tests**: approximately 40+.
**Expected workspace test count after**: ~1,660+ (from current ~1,598).

---

## Implementation Order

### Phase 1: Foundation (blocks everything)
1. P2.8 — Add `getrandom` crate, fix `generate_random_token`.
2. P1.8 — Replace custom percent-encoding with `percent-encoding` crate.
3. P0.1 — Restrict credentials file permissions.
4. P1.2 — Add `OAuthFlowStore` with state validation.

### Phase 2: Staging Hardening
5. P1.3 — Validate `task_id` in `shared_staging.rs`.
6. P1.4 — Atomic writes in `shared_staging.rs`.
7. P2.1 + P2.2 — Add `StagingLockGuard` + auto-sweep.
8. P2.3 — Fix test hardcoded path.

### Phase 3: Policy & Auth
9. P1.1 — OAuth state validation (completes P1.2 + P1.8 + P0.1).
10. P1.5 — MessageBus publisher authentication.
11. P1.6 — AuditLog path validation.
12. P2.10 + P2.11 — Fix policy engine placeholders.

### Phase 4: Periphery
13. P1.7 — Python client binary validation.
14. P1.9 — SecretScrubber short secret fix.
15. P2.4 — ConflictDetector clock handling.
16. P2.5 — MergeEngine deletion handling.
17. P2.6 — MessageBus history scrubbing.
18. P2.7 — OAuth refresh scope handling.
19. P2.9 — Credential file permissions.
20. P2.12 — Python adapter import errors.

### Phase 5: Distribution / Deployment Hardening
21. P3.9 — Add checksum verification to `download.sh`.
22. P3.10 — Add checksum verification to `install.js`.
23. P3.11 — Dockerfile builder stage user restriction.
24. P3.12 — Percent-decode error handling via `percent-encoding`.

### Phase 6: P3 Polish
25. P3.1-8 — Cross-cutting cleanup.

---

*Plan generated: 2026-04-29*
*Review status: Awaiting second pass for inconsistencies*

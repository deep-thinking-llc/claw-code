# TUI Shippable Milestone Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the current TUI branch a coherent shippable milestone by improving curated slash completion, aligning docs, and reconciling stale TUI planning notes.

**Architecture:** Keep slash completion candidates centralized in `rust/crates/ninmu-cli/src/format/slash_help.rs`, with `ratatui_app.rs` continuing to own fuzzy matching, preview rendering, and Tab cycling. Documentation changes stay in the existing TUI guide and enhancement plan rather than introducing a new roadmap.

**Tech Stack:** Rust, `cargo test`, `cargo fmt`, existing `ninmu-cli` TUI and formatting modules.

---

### Task 1: Completion Candidate Tests

**Files:**
- Modify: `rust/crates/ninmu-cli/src/format/slash_help.rs`

- [ ] **Step 1: Add failing tests for richer curated completions**

Add tests in the existing `#[cfg(test)]` module for `slash_command_completion_candidates_with_sessions()`:

```rust
#[test]
fn slash_completion_candidates_include_common_arguments() {
    let completions =
        slash_command_completion_candidates_with_sessions("sonnet", None, Vec::new());

    for expected in [
        "/config status",
        "/config get ",
        "/config set ",
        "/mcp add ",
        "/mcp remove ",
        "/mcp enable ",
        "/mcp disable ",
        "/plugin status",
        "/skills list",
        "/skills install ",
        "/agents list",
        "/agents run ",
        "/teleport status",
        "/pr status",
        "/issue status",
    ] {
        assert!(
            completions.contains(&expected.to_string()),
            "missing completion candidate: {expected}"
        );
    }
}

#[test]
fn slash_completion_candidates_keep_dynamic_model_and_sessions() {
    let completions = slash_command_completion_candidates_with_sessions(
        "openai/gpt-4.1",
        Some("active-session"),
        vec!["recent-a".to_string(), "recent-b".to_string()],
    );

    for expected in [
        "/model openai/gpt-4.1",
        "/resume active-session",
        "/session switch active-session",
        "/resume recent-a",
        "/session switch recent-b",
    ] {
        assert!(
            completions.contains(&expected.to_string()),
            "missing dynamic completion candidate: {expected}"
        );
    }
}
```

- [ ] **Step 2: Run tests and confirm RED**

Run:

```bash
rtk cargo test -p ninmu-cli slash_completion_candidates -- --nocapture
```

Expected: the new common-argument test fails because those candidates are not present yet.

### Task 2: Completion Candidate Implementation

**Files:**
- Modify: `rust/crates/ninmu-cli/src/format/slash_help.rs`
- Modify if needed: `rust/crates/ninmu-cli/src/tui/ratatui_app.rs`

- [ ] **Step 1: Add curated candidates**

Extend the static candidate array in `slash_command_completion_candidates_with_sessions()` with common argument forms for config, MCP, plugins, skills, agents, teleport, PR, and issue commands.

- [ ] **Step 2: Run focused tests and confirm GREEN**

Run:

```bash
rtk cargo test -p ninmu-cli slash_completion_candidates -- --nocapture
rtk cargo test -p ninmu-cli slash_completion -- --nocapture
```

Expected: all completion candidate and TUI slash-completion tests pass.

### Task 3: TUI Plan Reconciliation

**Files:**
- Modify: `rust/TUI-ENHANCEMENT-PLAN.md`

- [ ] **Step 1: Update current-state language**

Revise the "Weaknesses & Gaps" and phase tables so they describe landed, partial, and remaining work accurately. Keep the broad backlog visible, but mark status/HUD, thinking display, progress, collapsible tool output, pager, colored diff rendering, command palette/model selector, and curated slash completion as done or partial based on current code.

- [ ] **Step 2: Keep follow-up scope explicit**

Leave large remaining items as future work: structural extraction, richer live markdown, exhaustive command arguments, theme expansion, mouse support, search, undo, and broader navigation polish.

### Task 4: User Guide Alignment

**Files:**
- Modify: `docs/TUI-USER-GUIDE.md`

- [ ] **Step 1: Clarify slash completion**

Update the input dock and keybinding sections to state that Tab completes slash commands and common curated arguments.

- [ ] **Step 2: Add limitation note**

Add a short note near the command palette or slash-command section explaining that completion is curated and does not scan every possible filepath, model source, or external provider state.

### Task 5: Verification

**Files:**
- No source edits expected.

- [ ] **Step 1: Format**

Run:

```bash
rtk cargo fmt
```

- [ ] **Step 2: Focused tests**

Run:

```bash
rtk cargo test -p ninmu-cli slash_completion -- --nocapture
rtk cargo test -p ninmu-cli slash_completion_candidates -- --nocapture
```

- [ ] **Step 3: Broader CLI tests**

Run:

```bash
rtk cargo test -p ninmu-cli
```

Expected: tests pass. If any unrelated failure appears, capture the exact failing test and reason.

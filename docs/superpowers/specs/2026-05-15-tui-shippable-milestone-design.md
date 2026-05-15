# TUI Shippable Milestone Design

## Goal

Finish the next shippable TUI milestone by reconciling the stale enhancement plan with the current code, closing the obvious slash-completion gaps, aligning user-facing docs, and verifying the edited Rust surface.

## Scope

This pass treats the existing `ratatui` TUI as the active user interface. It does not attempt a broad rewrite, module extraction, theme expansion, mouse support, conversation search, undo, or other independent backlog items.

## Current State

The old TUI enhancement plan still describes several items as future work even though code now exists for them under `rust/crates/ninmu-cli/src/tui/`, including status and metadata display, thinking state, progress display, tool output handling, pager support, diff rendering, and collapsible scrollback. The current branch also added inline slash command completion in `ratatui_app.rs`, backed by candidate generation in `format/slash_help.rs`.

## Design

1. Reconcile `rust/TUI-ENHANCEMENT-PLAN.md` with the current implementation.
   - Mark already-landed TUI pieces as done or partially done.
   - Keep genuine follow-up work visible without implying the current code lacks shipped features.
   - Avoid rewriting the plan into a new roadmap format.

2. Improve slash-command argument completion without changing command dispatch.
   - Keep completion candidate generation centralized in `format/slash_help.rs`.
   - Add focused, static-but-useful candidates for commands that currently have obvious argument shapes.
   - Preserve existing fuzzy matching and Tab cycling behavior in `ratatui_app.rs`.
   - Do not introduce filesystem scanning or async provider discovery in this pass.

3. Align `docs/TUI-USER-GUIDE.md` with current behavior.
   - Document slash completion as command and common-argument completion.
   - Keep limitations explicit where completion remains curated rather than exhaustive.
   - Avoid promising later backlog items such as mouse support, conversation search, or undo.

4. Verify with focused Rust tests first.
   - Add or update tests around completion candidates and TUI completion behavior.
   - Run targeted `ninmu-cli` tests for completion and affected formatting.
   - Run formatting and broader Rust verification if the touched surface warrants it.

## Acceptance Criteria

- `rust/TUI-ENHANCEMENT-PLAN.md` no longer presents already-landed TUI modules as wholly missing.
- Slash command completion includes more useful common arguments while preserving existing behavior.
- `docs/TUI-USER-GUIDE.md` describes current completion behavior accurately.
- Focused Rust tests cover the completion additions.
- Verification commands complete successfully, or any remaining failures are reported with concrete output.

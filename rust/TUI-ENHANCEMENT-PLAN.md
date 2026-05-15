# TUI Enhancement Plan — Ninmu Code (`ninmu-cli`)

## Executive Summary

This plan covers a comprehensive analysis of the current terminal user interface and proposes phased enhancements that will transform the existing REPL/prompt CLI into a polished, modern TUI experience — while preserving the existing clean architecture and test coverage.

---

## 1. Current Architecture Analysis

### Crate Map

| Crate | Purpose | Lines | TUI Relevance |
|---|---|---|---|
| `ninmu-cli` | Main binary: REPL loop, arg parsing, rendering, API bridge | ~3,600 | **Primary TUI surface** |
| `runtime` | Session, conversation loop, config, permissions, compaction | ~5,300 | Provides data/state |
| `api` | Anthropic HTTP client + SSE streaming | ~1,500 | Provides stream events |
| `commands` | Slash command metadata/parsing/help | ~470 | Drives command dispatch |
| `tools` | 18 built-in tool implementations | ~3,500 | Tool execution display |

### Current TUI Components

> Note: The legacy prototype files `app.rs` and `args.rs` were removed on 2026-04-05.
> References below describe future extraction targets, not current tracked source files.

| Component | File | What It Does Today | Quality |
|---|---|---|---|
| **Input** | `input.rs` (269 lines) | `rustyline`-based line editor with slash-command tab completion, Shift+Enter newline, history | ✅ Solid |
| **Rendering** | `render.rs` (641 lines) | Markdown→terminal rendering (headings, lists, tables, code blocks with syntect highlighting, blockquotes), spinner widget | ✅ Good |
| **App/REPL loop** | `main.rs` (3,159 lines) | The monolithic `LiveCli` struct: REPL loop, all slash command handlers, streaming output, tool call display, permission prompting, session management | ⚠️ Monolithic |

### Key Dependencies

- **crossterm 0.28** — terminal control (cursor, colors, clear)
- **pulldown-cmark 0.13** — Markdown parsing
- **syntect 5** — syntax highlighting
- **rustyline 15** — line editing with completion
- **serde_json** — tool I/O formatting

### Strengths

1. **Clean rendering pipeline**: Markdown rendering is well-structured with state tracking, table rendering, code highlighting
2. **Rich tool display**: Tool calls get box-drawing borders (`╭─ name ─╮`), results show ✓/✗ icons
3. **Comprehensive slash commands**: 15 commands covering model switching, permissions, sessions, config, diff, export
4. **Session management**: Full persistence, resume, list, switch, compaction
5. **Permission prompting**: Interactive Y/N approval for restricted tool calls
6. **Thorough tests**: Every formatting function, every parse path has unit tests

### Landed TUI Capabilities

> Updated 2026-05-15: the original plan predated the active `ratatui` TUI. Several items below have since landed under `crates/ninmu-cli/src/tui/`; this document now tracks the remaining gaps rather than treating the whole TUI as future work.

1. **Full-screen TUI shell** — `tui/ratatui_app.rs` provides the active alternate-screen cockpit with header rail, conversation view, OPS panel, and input dock.
2. **Status and metadata display** — `tui/status_bar.rs`, `tui/progress.rs`, and the OPS panel expose model, permission, token/context, cost, branch, and progress metadata where available.
3. **Reasoning controls and thinking state** — `Ctrl+R`, `/effort`, `/think`, and `tui/thinking.rs` expose reasoning effort and thinking mode.
4. **Tool output readability** — `tui/scrollback.rs`, `tui/tool_panel.rs`, and `tui/timeline.rs` support collapsed long tool output, tool summaries, and visual tool state.
5. **Diff rendering and long-output paging** — `tui/diff_view.rs` renders colored diff summaries, and `tui/pager.rs` backs long command output.
6. **Command discovery** — `Ctrl+K` opens a command palette, `Ctrl+O` opens the model selector, and Tab now provides curated slash-command and common-argument completion.

### Remaining Gaps

1. **Structural extraction remains incomplete** — the inline CLI still carries substantial `LiveCli` responsibilities in `app.rs`; future work should keep module boundaries deliberate.
2. **Live streamed markdown is still limited** — richer incremental markdown rendering for assistant response streams remains a follow-up.
3. **Attachment previews are still absent** — resolved image/attachment inputs are not displayed inline in the TUI.
4. **Theme customization is limited** — the semantic TUI palette exists, but named user-selectable themes and color-depth fallback are not complete.
5. **Advanced navigation remains future work** — conversation search, undo, mouse support, and richer interactive session picking are still independent backlog items.
6. **Slash completion is curated** — common command arguments, model/session values, and aliases complete, but exhaustive filepath/provider/tool-state completion is not implemented.

---

## 2. Enhancement Plan

### Phase 0: Structural Cleanup (Foundation)

**Goal**: Break the monolith, remove dead code, establish the module structure for TUI work.

| Task | Description | Effort |
|---|---|---|
| 0.1 | **Extract `LiveCli` into `app.rs`** — Move the entire `LiveCli` struct, its impl, and helpers (`format_*`, `render_*`, session management) out of `main.rs` into focused modules: `app.rs` (core), `format.rs` (report formatting), `session_manager.rs` (session CRUD) | M |
| 0.2 | **Keep the legacy `CliApp` removed** — The old `CliApp` prototype has already been deleted; if any unique ideas remain valuable (for example stream event handler patterns), reintroduce them intentionally inside the active `LiveCli` extraction rather than restoring the old file wholesale | S |
| 0.3 | **Extract `main.rs` arg parsing** — The current `parse_args()` is still a hand-rolled parser in `main.rs`. If parsing is extracted later, do it into a newly-introduced module intentionally rather than reviving the removed prototype `args.rs` by accident | S |
| 0.4 | **Create a `tui/` module** — Done. The active TUI namespace includes `ratatui_app.rs`, `status_bar.rs`, `tool_panel.rs`, `diff_view.rs`, `pager.rs`, `theme.rs`, `progress.rs`, `thinking.rs`, and related helpers. | Done |

### Phase 1: Status Bar & Live HUD

**Goal**: Persistent information display during interaction.

| Task | Description | Effort |
|---|---|---|
| 1.1 | **Terminal-size-aware status line** — Done in the ratatui layout/header and metadata rail. | Done |
| 1.2 | **Live token counter** — Partially done through token/context metadata updates; continue refining provider-specific live usage fidelity as runtime events evolve. | Partial |
| 1.3 | **Turn duration timer** — Done for active turn/progress display where elapsed runtime is available. | Done |
| 1.4 | **Git branch indicator** — Done in the header rail when git metadata is available. | Done |

### Phase 2: Enhanced Streaming Output

**Goal**: Make the main response stream visually rich and responsive.

| Task | Description | Effort |
|---|---|---|
| 2.1 | **Live markdown rendering** — Still open for richer incremental markdown during assistant response streaming. | L |
| 2.2 | **Thinking indicator** — Done through TUI thinking state and reasoning controls. | Done |
| 2.3 | **Streaming progress bar** — Done for available progress and usage metadata; keep improving accuracy as providers expose richer usage events. | Done |
| 2.4 | **Remove artificial stream delay** — The current `stream_markdown` sleeps 8ms per chunk. For tool results this is fine, but for the main response stream it should be immediate or configurable | S |

### Phase 3: Tool Call Visualization

**Goal**: Make tool execution legible and navigable.

| Task | Description | Effort |
|---|---|---|
| 3.1 | **Collapsible tool output** — Done through scrollback collapsible entries and Tab toggling. | Done |
| 3.2 | **Syntax-highlighted tool results** — Partial. Markdown/code rendering exists, but not every tool output path is semantically highlighted. | Partial |
| 3.3 | **Tool call timeline** — Done through TUI tool/timeline state. | Done |
| 3.4 | **Diff-aware edit_file display** — Partial. Colored diff rendering exists; keep expanding edit-specific integration as tool result payloads allow. | Partial |
| 3.5 | **Permission prompt enhancement** — Done for TUI permission overlay styling. | Done |

### Phase 4: Enhanced Slash Commands & Navigation

**Goal**: Improve information display and add missing features.

| Task | Description | Effort |
|---|---|---|
| 4.1 | **Colored `/diff` output** — Done through `tui/diff_view.rs` and CLI diff report integration. | Done |
| 4.2 | **Pager for long outputs** — Done through `tui/pager.rs` and `print_with_pager`. | Done |
| 4.3 | **`/search` command** — Add a new command to search conversation history by keyword | M |
| 4.4 | **`/undo` command** — Undo the last file edit by restoring from the `originalFile` data in `write_file`/`edit_file` tool results | M |
| 4.5 | **Interactive session picker** — Still open; current session workflows use command palette entries plus slash commands. | L |
| 4.6 | **Tab completion for tool arguments** — Partial. Curated command arguments, model aliases/current model, active session, and recent sessions complete; exhaustive filepath and provider-state completion remain future work. | Partial |

### Phase 5: Color Themes & Configuration

**Goal**: User-customizable visual appearance.

| Task | Description | Effort |
|---|---|---|
| 5.1 | **Named color themes** — Add `dark` (current default), `light`, `solarized`, `catppuccin` themes. Wire to the existing `Config` tool's `theme` setting | M |
| 5.2 | **ANSI-256 / truecolor detection** — Detect terminal capabilities and fall back gracefully (no colors → 16 colors → 256 → truecolor) | M |
| 5.3 | **Configurable spinner style** — Allow choosing between braille dots, bar, moon phases, etc. | S |
| 5.4 | **Banner customization** — Make the ASCII art banner optional or configurable via settings | S |

### Phase 6: Full-Screen TUI Mode (Stretch)

**Goal**: Optional alternate-screen layout for power users.

| Task | Description | Effort |
|---|---|---|
| 6.1 | **Add `ratatui` dependency** — Done. | Done |
| 6.2 | **Split-pane layout** — Done for the active header/conversation/input/OPS layout. | Done |
| 6.3 | **Scrollable conversation view** — Partial. Scrolling exists; search remains open. | Partial |
| 6.4 | **Keyboard shortcuts panel** — Done through `?` / `F1` help overlay. | Done |
| 6.5 | **Mouse support** — Click to expand tool results, scroll conversation, select text for copy | L |

---

## 3. Priority Recommendation

### Immediate (High Impact, Moderate Effort)

1. **Phase 0.1 / 0.3** — Continue structural cleanup only when it directly reduces active maintenance risk.
2. **Phase 2.1** — Improve live streamed markdown rendering for assistant responses.
3. **Phase 3.2 / 3.4** — Finish semantic highlighting and edit-specific diff integration across all tool result paths.
4. **Phase 4.6** — Extend completion beyond curated arguments when the data source is cheap and deterministic.

### Near-Term (Next Sprint)

5. **Phase 4.3** — Conversation search.
6. **Phase 4.4** — Undo for the last file edit.
7. **Phase 4.5** — Interactive session picker.
8. **Phase 5.1–5.2** — Named themes and terminal color-depth fallback.

### Longer-Term

9. **Phase 5.3–5.4** — Spinner and banner customization.
10. **Phase 6.5** — Mouse support.
11. **Attachment previews** — Add image/attachment display once runtime payloads expose enough metadata.

---

## 4. Architecture Recommendations

### Module Structure After Phase 0

```
crates/ninmu-cli/src/
├── main.rs              # Entrypoint, arg dispatch only (~100 lines)
├── args.rs              # CLI argument parsing (consolidate existing two parsers)
├── app.rs               # LiveCli struct, REPL loop, turn execution
├── format.rs            # All report formatting (status, cost, model, permissions, etc.)
├── session_mgr.rs       # Session CRUD: create, resume, list, switch, persist
├── init.rs              # Repo initialization (unchanged)
├── input.rs             # Line editor (unchanged, minor extensions)
├── render.rs            # TerminalRenderer, Spinner (extended)
└── tui/
    ├── mod.rs           # TUI module root
    ├── status_bar.rs    # Persistent bottom status line
    ├── tool_panel.rs    # Tool call visualization (boxes, timelines, collapsible)
    ├── diff_view.rs     # Colored diff rendering
    ├── pager.rs         # Internal pager for long outputs
    └── theme.rs         # Color theme definitions and selection
```

### Key Design Principles

1. **Keep the inline REPL as the default** — Full-screen TUI should be opt-in (`--tui` flag)
2. **Everything testable without a terminal** — All formatting functions take `&mut impl Write`, never assume stdout directly
3. **Streaming-first** — Rendering should work incrementally, not buffering the entire response
4. **Respect `crossterm` for all terminal control** — Don't mix raw ANSI escape codes with crossterm (the current codebase does this in the startup banner)
5. **Feature-gate heavy dependencies** — `ratatui` should be behind a `full-tui` feature flag

---

## 5. Risk Assessment

| Risk | Mitigation |
|---|---|
| Breaking the working REPL during refactor | Phase 0 is pure restructuring with existing test coverage as safety net |
| Terminal compatibility issues (tmux, SSH, Windows) | Rely on crossterm's abstraction; test in degraded environments |
| Performance regression with rich rendering | Profile before/after; keep the fast path (raw streaming) always available |
| Scope creep into Phase 6 | Ship Phases 0–3 as a coherent release before starting Phase 6 |
| Historical `app.rs` vs `main.rs` confusion | Keep the legacy prototype removed and avoid reintroducing a second app surface accidentally during extraction |

---

*Generated: 2026-03-31 | Workspace: `rust/` | Branch: `dev/rust`*

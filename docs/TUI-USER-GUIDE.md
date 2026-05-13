# Ninmu TUI User Guide

The Ninmu terminal UI is the human-friendly way to work with Ninmu Code. It keeps the agent-first CLI intact for scripts and harnesses, while giving humans a richer full-screen console for steering models, approving tools, reading output, and resuming sessions.

Use `--tui` when you want an interactive coding cockpit. Leave `--tui` off when you want lightweight machine-readable CLI behavior.

---

## Quick Start

From a built binary:

```bash
ninmu --tui
```

From source:

```bash
cd src
cargo run -p ninmu-cli -- --tui
```

Start with a specific model:

```bash
ninmu --model sonnet --tui
ninmu --model openai/gpt-4.1 --tui
ninmu --model ollama/llama3.1:8b --tui
```

Start with a specific permission mode:

```bash
ninmu --permission-mode read-only --tui
ninmu --permission-mode workspace-write --tui
ninmu --permission-mode danger-full-access --tui
```

Resume an existing session:

```bash
ninmu --resume latest --tui
```

Inside the TUI, press `?` or `F1` at any time to show the built-in help overlay.

---

## When to Use TUI Mode

Use `--tui` for:

- Human-guided coding sessions.
- Reviewing and approving tool calls.
- Switching models, reasoning effort, or permission modes mid-session.
- Reading long agent responses with scrolling, folding, and ANSI color rendering.
- Resuming earlier sessions and continuing interactively.

Avoid `--tui` for:

- Agentic harnesses that call Ninmu programmatically.
- CI jobs, shell scripts, and cron tasks.
- JSON output pipelines.
- Low-memory non-interactive jobs.

For automation, prefer the standard CLI:

```bash
ninmu --output-format json status
ninmu prompt "summarize the current git diff"
ninmu --resume latest /status /diff /export notes.txt
```

The non-TUI path avoids initializing the full-screen renderer and is intended to stay lightweight.

---

## Screen Layout

The TUI is divided into four main areas.

### Header Rail

The header shows the current operating state:

| Field | Meaning |
|-------|---------|
| `MODEL` | Active model or model alias. |
| `PERM` | Current permission mode. |
| `BRANCH` | Current git branch when available. |
| `THINK` | Thinking mode: `auto`, `on`, or `off`. |
| `EFFORT` | Reasoning effort: default, low, medium, high, or max. |

### Conversation View

The main scrollback contains user prompts, assistant responses, tool calls, tool results, status messages, and errors. It supports scrolling and folded tool output so long sessions remain readable.

### OPS Panel

On wide terminals, the right-side OPS panel summarizes live operational state:

- Current model.
- Current or recent tool.
- Permission mode.
- Token and context information when available.
- Cost or pricing metadata when available.
- Model capability metadata when available.

On narrow terminals, the same information is condensed into the main layout and footer.

### Input Dock

The input dock is where you write prompts and slash commands. It supports cursor movement, prompt history, multi-line prompts, and slash command completion.

---

## Essential Keybindings

| Key | Action |
|-----|--------|
| `Enter` | Submit the current prompt or command. |
| `Ctrl+Enter` | Insert a newline into the input. |
| `Ctrl+K` | Open the command palette. |
| `Ctrl+R` | Open reasoning and thinking controls. |
| `Ctrl+O` | Open the model selector. |
| `?` or `F1` | Toggle help. |
| `Tab` | Complete slash commands, or expand/collapse tool output. |
| `Up` / `Down` | Navigate input history or overlay lists. |
| `Left` / `Right` | Move the cursor, or adjust values in selectors. |
| `PageUp` / `PageDown` | Scroll the conversation. |
| `Home` / `End` | Move to input start/end, or scroll top/bottom while generation is active. |
| `Esc` | Close an overlay, cancel generation, or deny a permission prompt. |
| `Ctrl+C` / `Ctrl+D` | Quit the TUI. |

The same keys adapt to the active overlay. For example, `Up` and `Down` browse prompt history in the input dock, but move through choices when the model selector is open.

---

## Command Palette

Open the command palette with `Ctrl+K`.

The palette is the fastest way to discover and run common actions without memorizing every slash command.

Common actions include:

- Open reasoning controls.
- Open model selector.
- Show help.
- Show stats.
- Inspect permissions.
- Change permission mode.
- List sessions.
- Resume a session.
- Show prompt history.
- Export the conversation.
- Clear the transcript.
- Start a fresh session.

How to use it:

1. Press `Ctrl+K`.
2. Type a few letters to filter the list.
3. Use `Up` and `Down` to choose an action.
4. Press `Enter` to run it.
5. Press `Esc` to close without running anything.

Some palette entries run immediately, such as stats or help. Others insert a slash command into the input so you can finish the argument yourself, such as `/export ` or `/permissions `.

---

## Reasoning And Thinking Controls

Open the reasoning selector with `Ctrl+R`.

This overlay controls two related settings:

| Setting | Values | Purpose |
|---------|--------|---------|
| Reasoning effort | default, low, medium, high, max | Sets how much effort compatible models should spend on reasoning. |
| Thinking mode | auto, on, off | Controls whether compatible models expose thinking blocks. |

Keyboard controls:

| Key | Action |
|-----|--------|
| `Up` / `Down` | Switch between effort and thinking rows. |
| `Left` / `Right` | Change the selected value. |
| `1` to `5` | Select an effort level quickly. |
| `a` | Set thinking to auto. |
| `o` | Turn thinking on. |
| `f` | Turn thinking off. |
| `Enter` | Apply the highlighted row. |
| `Esc` | Close without applying. |

You can also use slash commands:

```text
/effort low
/effort medium
/effort high
/effort max
/effort off
/think auto
/think on
/think off
```

Provider support varies. If a model or provider does not support one of these settings, Ninmu keeps the UI state visible but the provider may ignore or normalize the value.

### Scenario: Use More Reasoning For A Hard Refactor

1. Press `Ctrl+R`.
2. Select `EFFORT`.
3. Move to `high` or `max`.
4. Press `Enter`.
5. Ask the model to inspect the design before editing:

```text
Review the authentication flow first, then propose a minimal implementation plan before changing files.
```

After the hard part is done, set effort back to default or low to reduce latency and cost.

---

## Model Selector

Open the model selector with `Ctrl+O`, or type `/model`.

The selector shows available model entries with provider and capability metadata.

| Column | Meaning |
|--------|---------|
| Provider | Anthropic, OpenAI, xAI, DeepSeek, DashScope, Ollama, vLLM, Qwen, or custom. |
| Context | Approximate context window when known. |
| Family | Provider or model family when known. |
| Price | Pricing metadata when available. |
| Capabilities | Known capabilities such as tool use, vision, or reasoning. |
| Status | Whether the model is current, ready, or missing credentials. |

Keyboard controls:

| Key | Action |
|-----|--------|
| Type text | Filter models. |
| `Up` / `Down` | Move through matches. |
| `Enter` | Select the highlighted model. |
| `Tab` | Cycle provider filters. |
| `Shift+Tab` | Clear the provider filter. |
| `Esc` | Close the selector. |

Direct slash command:

```text
/model sonnet
/model openai/gpt-4.1
/model ollama/llama3.1:8b
```

If a model shows `KEY REQUIRED`, set the matching provider key before using it. Examples:

```bash
export ANTHROPIC_API_KEY="sk-ant-..."
export OPENAI_API_KEY="sk-..."
export DEEPSEEK_API_KEY="sk-..."
```

Local models such as Ollama and vLLM usually require a local server instead of a hosted API key.

### Scenario: Switch From Local To Hosted

1. Start locally:

```bash
ninmu --model ollama/llama3.1:8b --tui
```

2. Work through cheap exploration or repository reading.
3. Press `Ctrl+O`.
4. Filter for `sonnet`, `openai`, or another hosted model.
5. Press `Enter`.
6. Continue the same session with the new model.

This is useful when you want a local model for broad inspection and a stronger hosted model for final implementation or review.

---

## Tool Output

When the assistant uses tools, the TUI records both the tool call and the result.

Tool results are designed to stay readable:

- Results show success or failure state.
- Long output is summarized.
- ANSI color sequences are rendered instead of printed raw.
- Code fences get lightweight syntax styling.
- Very long results are capped in the visible UI to protect readability.
- The footer and OPS panel show the current or most recent tool.

Use `Tab` to expand or collapse tool output when the input is not completing a slash command.

### Scenario: Inspect A Failed Test

1. Ask Ninmu to run or fix tests.
2. When a test tool result appears, press `Tab` to expand it.
3. Use `PageUp` and `PageDown` to scan the failure.
4. Ask a targeted follow-up:

```text
The failure is in the parser snapshot. Explain whether this is a real regression or a snapshot update.
```

The goal is to keep the main conversation clean while still letting you inspect the exact output when it matters.

---

## Permissions

Ninmu uses permission modes to control what tools may do.

| Mode | Intended use |
|------|--------------|
| `read-only` | Inspect code, summarize files, and run safe read-only operations. |
| `workspace-write` | Edit files inside the workspace and run normal development commands. |
| `danger-full-access` | Allow broad filesystem and command access. Use only when you trust the task. |

Inspect or change permissions:

```text
/permissions
/permissions read-only
/permissions workspace-write
/permissions danger-full-access
```

Permission prompts appear when a tool action needs approval. Prompts include risk labels so you can decide quickly.

Common risk labels:

| Label | Meaning |
|-------|---------|
| `READ` | Reads files or repository state. |
| `WRITE` | Writes or modifies files. |
| `EXEC` | Runs a command. |
| `NETWORK` | Uses network access. |
| `BROAD CWD` | Operates outside a narrow file or workspace target. |

Prompt controls:

| Key | Action |
|-----|--------|
| `Y` or `A` | Allow the requested action. |
| `N` or `D` | Deny the requested action. |
| `V` | View more input or command detail. |
| `Esc` | Deny and close. |

### Scenario: Work Safely In An Unknown Repository

1. Start read-only:

```bash
ninmu --permission-mode read-only --tui
```

2. Ask for inspection only:

```text
Map the repository structure and identify the test commands, but do not edit files yet.
```

3. When you are ready to let Ninmu edit:

```text
/permissions workspace-write
```

4. Ask for a focused implementation.

This workflow keeps exploration safe, then opens write access only after you understand the plan.

---

## Slash Commands

Slash commands work inside the TUI input dock. Type `/` and press `Tab` to complete available commands.

Common commands:

| Command | Purpose |
|---------|---------|
| `/help` | Show command help. |
| `/model` | Inspect the current model or open model switching flow. |
| `/model <name>` | Switch model. |
| `/effort <level>` | Set reasoning effort. |
| `/think <mode>` | Set thinking mode. |
| `/permissions` | Inspect permission mode. |
| `/permissions <mode>` | Change permission mode. |
| `/status` | Show current session and environment status. |
| `/stats` | Show session statistics. |
| `/doctor` | Run provider and environment checks. |
| `/diff` | Show current git diff. |
| `/commit` | Ask Ninmu to help prepare a commit. |
| `/history [count]` | Show recent prompt history. |
| `/resume <session>` | Resume a session by path, id, or `latest`. |
| `/session list` | List sessions. |
| `/export [file]` | Export conversation content. |
| `/clear --confirm` | Start a fresh session. |

Some slash commands are intended for active interactive sessions. Others can also be used with non-TUI resume workflows:

```bash
ninmu --resume latest /status /diff /export notes.txt
```

---

## Writing Effective Prompts In The TUI

### Multi-line Prompts

Use `Ctrl+Enter` to insert a newline:

```text
Please implement the feature with these constraints:

1. Keep the non-TUI CLI lightweight.
2. Add tests for the TUI path.
3. Do not refactor unrelated modules.
```

Press `Enter` when you are ready to send.

### Prompt History

Use `Up` and `Down` from the input dock to revisit previous prompts. This is useful when you want to adjust a recent instruction without retyping it.

### File And Command References

Be explicit when you care about scope:

```text
Only inspect src/crates/ninmu-cli/src/tui and propose changes before editing.
```

```text
Run the narrow TUI tests first, then broaden only if the narrow tests pass.
```

Clear constraints make permission prompts easier to evaluate and help keep edits focused.

---

## Tutorials

### Tutorial 1: First TUI Session

1. Configure a provider:

```bash
export ANTHROPIC_API_KEY="sk-ant-..."
```

2. Start Ninmu:

```bash
ninmu --model sonnet --permission-mode workspace-write --tui
```

3. Press `?` to review the help overlay.
4. Ask for a repository tour:

```text
Explain this repository at a high level, then identify the safest first test command.
```

5. If Ninmu asks to run a command, inspect the permission prompt and allow only if it matches the task.

### Tutorial 2: Local Ollama Exploration

1. Start Ollama separately.
2. Launch Ninmu with a local model:

```bash
ninmu --model ollama/llama3.1:8b --permission-mode read-only --tui
```

3. Ask for read-only inspection:

```text
Scan the repository and summarize the main crates, but do not edit files.
```

4. Press `Ctrl+O` later if you want to switch to a hosted model for implementation.

### Tutorial 3: Change Reasoning Mid-session

1. Start normally:

```bash
ninmu --tui
```

2. Ask Ninmu to investigate a bug.
3. Before the final fix, press `Ctrl+R`.
4. Set effort to `high`.
5. Continue:

```text
Now implement the smallest fix and add tests that would have caught the bug.
```

6. After the fix, set effort back to default or low.

### Tutorial 4: Review A Permission Prompt

1. Ask Ninmu for an implementation.
2. When a permission prompt opens, read the command, target, and risk labels.
3. Press `V` if the displayed command is truncated.
4. Press `Y` only when the command matches your intent.
5. Press `N` or `Esc` if the command is too broad.

Good follow-up after denying:

```text
That command was too broad. Please run a narrower command limited to src/crates/ninmu-cli.
```

### Tutorial 5: Export A Session

1. Open the command palette with `Ctrl+K`.
2. Search for `export`.
3. Press `Enter`; the palette inserts `/export ` into the input.
4. Add a path:

```text
/export notes.txt
```

5. Press `Enter`.

Use exports for handoff notes, design records, and review summaries.

### Tutorial 6: Start Fresh Without Losing The Old Session

To clear the active conversation and start fresh:

```text
/clear --confirm
```

The clear flow preserves the old session reference so you can resume it later:

```text
/resume latest
```

or:

```text
/resume <session-id-or-path>
```

---

## Troubleshooting

### The TUI Does Not Open

Make sure you passed `--tui`:

```bash
ninmu --tui
```

If the terminal looks corrupted after an interrupted run, reset the terminal:

```bash
reset
```

Then start again in a terminal with enough height and width for the full layout.

### A Model Shows `KEY REQUIRED`

Set the provider API key or choose a local provider that does not require one.

```bash
export OPENAI_API_KEY="sk-..."
export ANTHROPIC_API_KEY="sk-ant-..."
```

Then reopen the TUI or switch models again.

### Ollama Or vLLM Does Not Respond

Confirm the local server is running and the base URL is correct:

```bash
export OLLAMA_BASE_URL="http://localhost:11434"
export VLLM_BASE_URL="http://localhost:8000/v1"
```

Then run:

```text
/doctor
```

### I Need JSON Or Scriptable Output

Do not use `--tui`. Use the standard CLI:

```bash
ninmu --output-format json status
ninmu --resume latest /status /diff
```

### Output Is Too Condensed

Press `Tab` to expand folded tool output. Use `PageUp`, `PageDown`, `Home`, and `End` to move through scrollback.

### I Want Less Resource Usage

Use non-TUI mode for automation and lightweight harnesses:

```bash
ninmu prompt "summarize this repository"
ninmu --output-format json status
```

The TUI is optimized for human interaction, not minimum memory footprint.

---

## Practical Workflows

### Exploration First, Editing Second

```bash
ninmu --permission-mode read-only --tui
```

Prompt:

```text
Inspect the repository and propose the smallest implementation plan. Do not edit files yet.
```

Then, when you approve the plan:

```text
/permissions workspace-write
```

Prompt:

```text
Implement the first step only, then run the narrowest relevant tests.
```

### Fast Local Read, Strong Hosted Finish

```bash
ninmu --model ollama/llama3.1:8b --permission-mode read-only --tui
```

Use the local model to explore. Then press `Ctrl+O`, switch to a hosted model, and continue:

```text
Now implement the selected fix and explain the verification.
```

### High-Rigor Review

1. Press `Ctrl+R`.
2. Set effort to `high` or `max`.
3. Ask for review:

```text
Review the current diff for correctness, regressions, missing tests, and risky assumptions. Findings first.
```

4. Use `Tab` to expand test or diff output when needed.
5. Set effort back to default when the review is complete.

---

## Tips

- Use `Ctrl+K` when you remember the action but not the command.
- Use `Ctrl+R` before planning, debugging, or reviewing hard changes.
- Use `Ctrl+O` when a task changes from cheap exploration to high-quality implementation.
- Use `read-only` until you are ready for edits.
- Use `workspace-write` for normal coding.
- Reserve `danger-full-access` for trusted tasks that genuinely need it.
- Use non-TUI mode for scripts, CI, and lightweight agentic harnesses.

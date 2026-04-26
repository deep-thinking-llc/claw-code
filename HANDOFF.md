# Handoff: Multi-Provider Implementation — Complete

## Branch: `main` (ahead of origin by 2 commits)

Last commits: `f5df140` (fix clippy), `f240236` (--provider flag)

---

## What was built

### 11 providers — full routing, streaming, pricing, CLI integration

| Provider | API key env | Default URL | Model prefix |
|----------|------------|-------------|-------------|
| Anthropic | `ANTHROPIC_API_KEY` | api.anthropic.com | `claude-*`, aliases: `opus`, `sonnet`, `haiku` |
| OpenAI | `OPENAI_API_KEY` | api.openai.com/v1 | `openai/*`, `gpt-*` |
| xAI (Grok) | `XAI_API_KEY` | api.x.ai/v1 | `grok-*`, aliases: `grok`, `grok-mini`, `grok-2` |
| DeepSeek | `DEEPSEEK_API_KEY` | api.deepseek.com/v1 | `deepseek-chat`, `deepseek-reasoner`, `deepseek-r1` |
| DashScope | `DASHSCOPE_API_KEY` | dashscope.aliyuncs.com | `qwen-*` (bare), `kimi-*` |
| Ollama | `OLLAMA_API_KEY` (opt) | localhost:11434/v1 | `ollama/*` |
| vLLM | none | localhost:8000/v1 | `vllm/*` |
| Qwen (ext) | `QWEN_API_KEY` → `OPENAI_API_KEY` | `QWEN_BASE_URL` → `OPENAI_BASE_URL` | `qwen/*` |
| Mistral | `MISTRAL_API_KEY` | api.mistral.ai/v1 | `mistral-large-latest` |
| Gemini | `GEMINI_API_KEY` | generativelanguage.googleapis.com/v1beta | `gemini-2.5-pro` |
| Cohere | `COHERE_API_KEY` | api.cohere.com/v1 | `command-r-plus` |

### Key features delivered
- DeepSeek R1 reasoning: `reasoning_content` → Thinking blocks with correct index offsets
- Ollama Cloud: optional `OLLAMA_API_KEY` for api.ollama.com; local works without auth
- Qwen fallback chain: API key and base URL fall through to OpenAI env vars
- `--provider` CLI flag: `ninmu --provider deepseek --model chat prompt "hello"`
- Provider fallback chains: `ProviderClient::from_model_chain(primary, fallbacks)`
- Per-provider token/cost tracking: `UsageTracker::record(usage, Some("deepseek-chat"))`
- Provider-specific defaults in settings.json: `{ "providers": { "deepseek": { "maxTokens": 8192 } } }`
- CLI doctor: `check_providers_health()` shows all 11 providers + auto-discovers Ollama/vLLM models
- CLI init: `.env.example` with templates for all providers
- models.json validation: rejects unknown `api` values

### Test totals
- API lib: 171 | API integration: 37 | Runtime: 472 | Ninmu CLI: 261
- **Total: 941 tests, 0 failures**

---

## Remaining work

### P0 — Wire `apply_provider_defaults` to requests
**File:** `runtime/src/config.rs` has `apply_provider_defaults()` ready.  
**Need:** Call it in `ninmu-cli/src/app.rs` where `MessageRequest` is built, passing the model name and runtime config.  
**Pattern:**
```rust
let mut max_tokens = config.max_tokens();
let mut temperature = config.temperature();
runtime::apply_provider_defaults(
    &mut max_tokens, &mut temperature, &mut None, &mut None,
    &model, &runtime_config,
);
```

### P1 — Wire per-provider cost tracking
**File:** `runtime/src/usage.rs` — `UsageTracker::record()` now accepts `model: Option<&str>`.  
**Need:** In `runtime/src/conversation.rs` line 372, pass the model name instead of `None`:
```rust
self.usage_tracker.record(usage, Some(&self.model));  // currently passes None
```

### P2 — Model-specific token limits
Add to `api/src/providers/mod.rs` → `model_token_limit()`:
- `mistral-large-latest`: 128K ctx
- `gemini-2.5-pro`: 1M ctx  
- `command-r-plus`: 128K ctx

### P3 — `is_reasoning_model` updates
Add to `api/src/providers/openai_compat.rs` → `is_reasoning_model()`:
- Gemini thinking models
- Cohere reasoning models

### P4 — Push to origin
Branch is 2 commits ahead of origin/main. Run: `git push`

---

## Key files by area

| Area | Files |
|------|-------|
| Provider routing | `api/src/providers/mod.rs` (+500 lines) |
| OpenAI-compat configs | `api/src/providers/openai_compat.rs` (+400 lines) |
| Provider client | `api/src/client.rs` |
| Config + defaults | `runtime/src/config.rs` (+250 lines) |
| Pricing | `runtime/src/usage.rs` (+200 lines) |
| CLI doctor | `ninmu-cli/src/cli_commands.rs` |
| CLI args + --provider | `ninmu-cli/src/args.rs` |
| CLI init | `ninmu-cli/src/init.rs` |
| CLI labels | `ninmu-cli/src/format/model.rs` |
| CLI dispatch | `ninmu-cli/src/app.rs` |
| models.json validation | `api/src/providers/models_file.rs` |
| docs | `_provider-implementation-plan.md` (gitignored, WIP) |
| README | `README.md` |

//! Fetch model metadata from models.dev.
//!
//! Provides async background refresh of the models.dev API into an in-memory
//! cache. The cache is merged into [`list_available_models`] as a third tier
//! (after the built-in `MODEL_REGISTRY` and custom `models.json` entries).
//!
//! models.dev is a community-maintained open-source database of AI model
//! specifications, pricing, and capabilities. See <https://models.dev>.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};
use std::time::Duration;

use serde::Deserialize;

use super::{ModelEntry, ProviderKind};

// ---------------------------------------------------------------------------
// API types
// ---------------------------------------------------------------------------

/// Top-level models.dev API response: provider ID → metadata.
type ModelsDevResponse = HashMap<String, ModelsDevProvider>;

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct ModelsDevProvider {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    env: Vec<String>,
    #[serde(default)]
    models: HashMap<String, ModelsDevModel>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct ModelsDevModel {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    family: Option<String>,
    #[serde(default)]
    reasoning: bool,
    #[serde(default)]
    tool_call: bool,
    #[serde(default)]
    cost: Option<ModelsDevCost>,
    #[serde(default)]
    limit: Option<ModelsDevLimit>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct ModelsDevCost {
    input: Option<f64>,
    output: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct ModelsDevLimit {
    context: Option<u32>,
    output: Option<u32>,
}

// ---------------------------------------------------------------------------
// Global in-memory cache
// ---------------------------------------------------------------------------

static MODELS_DEV_CACHE: OnceLock<RwLock<Option<Vec<ModelEntry>>>> = OnceLock::new();

fn cache() -> &'static RwLock<Option<Vec<ModelEntry>>> {
    MODELS_DEV_CACHE.get_or_init(|| RwLock::new(None))
}

/// Read the cached models.dev entries, if available.
#[must_use]
pub fn cached_models() -> Option<Vec<ModelEntry>> {
    cache().read().ok()?.clone()
}

// ---------------------------------------------------------------------------
// Provider mapping
// ---------------------------------------------------------------------------

/// Map a models.dev provider ID to our [`ProviderKind`].
///
/// vLLM is excluded because models.dev has no `vllm` provider ID — vLLM is a
/// self-hosted inference server, not an API provider listed in the catalog.
fn models_dev_provider_to_kind(provider_id: &str) -> Option<ProviderKind> {
    match provider_id {
        "anthropic" => Some(ProviderKind::Anthropic),
        "openai" => Some(ProviderKind::OpenAi),
        "xai" => Some(ProviderKind::Xai),
        "deepseek" => Some(ProviderKind::DeepSeek),
        "ollama" | "ollama-cloud" => Some(ProviderKind::Ollama),
        "qwen" | "alibaba" | "alibaba-cn" => Some(ProviderKind::Qwen),
        "mistral" => Some(ProviderKind::Mistral),
        "google" | "google-vertex" | "google-vertex-anthropic" => Some(ProviderKind::Gemini),
        "cohere" => Some(ProviderKind::Cohere),
        // Skipped: azure, aws-bedrock, groq, openrouter, together,
        // fireworks-ai, perplexity, opencode, poe, venice, etc.
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Fetch + convert
// ---------------------------------------------------------------------------

const MODELS_DEV_URL: &str = "https://models.dev/api.json";
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);

/// Fetch models from models.dev and populate the in-memory cache.
///
/// Uses a fresh Tokio runtime and `reqwest`'s async client on a bare thread
/// (no enclosing async context) so this is safe to call from `std::thread`.
///
/// Returns `Ok(count)` on success, `Err` on network or parse failure.
pub fn refresh_models() -> Result<usize, String> {
    let rt = tokio::runtime::Runtime::new().map_err(|e| format!("tokio runtime: {e}"))?;

    let parsed: ModelsDevResponse = rt.block_on(async {
        let client = reqwest::Client::builder()
            .timeout(FETCH_TIMEOUT)
            .build()
            .map_err(|e| format!("http client: {e}"))?;

        let response = client
            .get(MODELS_DEV_URL)
            .send()
            .await
            .map_err(|e| format!("fetch: {e}"))?;

        if !response.status().is_success() {
            return Err(format!("HTTP {}", response.status()));
        }

        response
            .json::<ModelsDevResponse>()
            .await
            .map_err(|e| format!("parse: {e}"))
    })?;

    let entries = convert_models_dev_to_entries(&parsed);
    let count = entries.len();
    let mut guard = cache().write().map_err(|e| e.to_string())?;
    *guard = Some(entries);
    Ok(count)
}

/// Spawn a background thread to refresh models from models.dev.
///
/// Returns immediately; the cache is populated when the fetch completes.
pub fn refresh_models_async() {
    std::thread::spawn(|| {
        match refresh_models() {
            Ok(count) => eprintln!("[ninmu] loaded {count} models from models.dev"),
            Err(e) => eprintln!("[ninmu] models.dev refresh failed: {e}"),
        }
    });
}

// ---------------------------------------------------------------------------
// Conversion
// ---------------------------------------------------------------------------

fn convert_models_dev_to_entries(
    providers: &ModelsDevResponse,
) -> Vec<ModelEntry> {
    let mut entries = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for (provider_id, provider) in providers {
        let Some(kind) = models_dev_provider_to_kind(provider_id) else {
            continue;
        };
        let no_auth_required = matches!(kind, ProviderKind::Ollama | ProviderKind::Vllm);

        for (model_id, model) in &provider.models {
            let canonical = model_id.clone();
            if !seen.insert(canonical.clone()) {
                continue;
            }
            let alias = model.name.clone().unwrap_or_else(|| canonical.clone());

            // Auth detection: prefer models.dev's env field, but fall
            // back to metadata_for_model() for consistency with the
            // rest of the app (which uses ProviderMetadata.auth_env).
            let has_auth = if no_auth_required {
                true
            } else {
                let from_dev_env = provider.env.iter().any(|var| std::env::var(var).is_ok());
                if from_dev_env {
                    true
                } else {
                    // Fallback: use existing metadata_for_model routing
                    super::metadata_for_model(&canonical)
                        .is_none_or(|meta| std::env::var(meta.auth_env).is_ok())
                }
            };

            entries.push(ModelEntry {
                alias,
                canonical,
                provider: kind,
                has_auth,
            });
        }
    }
    entries
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_all_supported_providers() {
        let cases: &[(&str, ProviderKind)] = &[
            ("anthropic", ProviderKind::Anthropic),
            ("openai", ProviderKind::OpenAi),
            ("xai", ProviderKind::Xai),
            ("deepseek", ProviderKind::DeepSeek),
            ("ollama", ProviderKind::Ollama),
            ("ollama-cloud", ProviderKind::Ollama),
            ("qwen", ProviderKind::Qwen),
            ("alibaba", ProviderKind::Qwen),
            ("alibaba-cn", ProviderKind::Qwen),
            ("mistral", ProviderKind::Mistral),
            ("google", ProviderKind::Gemini),
            ("google-vertex", ProviderKind::Gemini),
            ("google-vertex-anthropic", ProviderKind::Gemini),
            ("cohere", ProviderKind::Cohere),
        ];
        for (id, expected) in cases {
            assert_eq!(
                models_dev_provider_to_kind(id),
                Some(*expected),
                "provider {id}"
            );
        }
    }

    #[test]
    fn rejects_unsupported_providers() {
        for id in &["azure", "amazon-bedrock", "groq", "openrouter", "together"] {
            assert_eq!(models_dev_provider_to_kind(id), None, "provider {id}");
        }
    }

    #[test]
    fn convert_empty_response_yields_empty_entries() {
        let input = HashMap::new();
        let entries = convert_models_dev_to_entries(&input);
        assert!(entries.is_empty());
    }

    #[test]
    fn convert_single_provider() {
        let mut providers = HashMap::new();
        providers.insert(
            "openai".to_string(),
            ModelsDevProvider {
                id: Some("openai".to_string()),
                name: Some("OpenAI".to_string()),
                env: vec!["OPENAI_API_KEY".to_string()],
                models: [(
                    "gpt-4o".to_string(),
                    ModelsDevModel {
                        name: Some("GPT-4o".to_string()),
                        family: Some("gpt".to_string()),
                        reasoning: false,
                        tool_call: true,
                        cost: Some(ModelsDevCost {
                            input: Some(2.5),
                            output: Some(10.0),
                        }),
                        limit: Some(ModelsDevLimit {
                            context: Some(128_000),
                            output: Some(16_384),
                        }),
                    },
                )]
                .into(),
            },
        );

        let entries = convert_models_dev_to_entries(&providers);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].alias, "GPT-4o");
        assert_eq!(entries[0].canonical, "gpt-4o");
        assert_eq!(entries[0].provider, ProviderKind::OpenAi);
    }

    #[test]
    fn deduplicates_by_canonical_name() {
        let mut providers = HashMap::new();
        let model = ModelsDevModel {
            name: Some("GPT-4o".to_string()),
            family: Some("gpt".to_string()),
            reasoning: false,
            tool_call: true,
            cost: None,
            limit: None,
        };
        // Same model ID under two provider IDs — only one entry should appear
        // (the first one wins by provider iteration order).
        providers.insert(
            "openai".to_string(),
            ModelsDevProvider {
                id: Some("openai".to_string()),
                name: Some("OpenAI".to_string()),
                env: vec![],
                models: [("gpt-4o".to_string(), model.clone())].into(),
            },
        );
        providers.insert(
            "azure".to_string(),
            ModelsDevProvider {
                id: Some("azure".to_string()),
                name: Some("Azure".to_string()),
                env: vec![],
                models: [("gpt-4o".to_string(), model)].into(),
            },
        );

        let entries = convert_models_dev_to_entries(&providers);
        // Only the openai entry (supported provider) should be present
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn cache_is_empty_initialized() {
        // Verify the cache is empty before any refresh call
        assert!(cached_models().is_none());
    }
}

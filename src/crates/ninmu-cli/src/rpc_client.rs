//! Minimal API client for the JSON-RPC server.
//!
//! `RpcApiClient` implements `ninmu_runtime::ApiClient` by delegating to
//! `ninmu_api::ProviderClient`.  It is intentionally stripped-down:
//! no markdown rendering, no TUI bridging, no progress reporting — just
//! raw API calls and event conversion.

use ninmu_api::{
    MessageRequest, MessageResponse, OutputContentBlock, ProviderClient, ToolChoice,
    ToolDefinition, ToolResultContentBlock, Usage,
};
use ninmu_runtime::{
    ApiClient, ApiRequest, AssistantEvent, ContentBlock, ConversationMessage, MessageRole,
    RuntimeError, TokenUsage,
};

/// A thin synchronous wrapper around `ProviderClient` for use in the
/// JSON-RPC server.
pub struct RpcApiClient {
    runtime: tokio::runtime::Runtime,
    client: ProviderClient,
    model: String,
    /// Tool definitions sent on every request. Empty = no tools (matches
    /// pre-existing behavior for callers that don't supply any). When set,
    /// these are sent verbatim — the registry should already have applied
    /// any allowlist filtering and alphabetical sort for cache stability.
    tools: Vec<ToolDefinition>,
}

impl RpcApiClient {
    /// Build a provider client for `model`.
    pub fn new(model: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let runtime = tokio::runtime::Runtime::new()?;
        let resolved_model = ninmu_api::resolve_model_alias(model);
        let client = runtime.block_on(async {
            match ninmu_api::detect_provider_kind(&resolved_model) {
                ninmu_api::ProviderKind::Anthropic => {
                    let auth = ninmu_api::resolve_startup_auth_source(|| Ok(None))?;
                    let inner = ninmu_api::AnthropicClient::from_auth(auth)
                        .with_base_url(ninmu_api::read_base_url());
                    Ok::<_, Box<dyn std::error::Error>>(ProviderClient::Anthropic(inner))
                }
                _ => Ok(ProviderClient::from_model_with_anthropic_auth(
                    &resolved_model,
                    None,
                )?),
            }
        })?;
        Ok(Self {
            runtime,
            client,
            model: resolved_model,
            tools: Vec::new(),
        })
    }

    /// Attach a sorted, allowlist-filtered tool list. Callers should pass
    /// the same tools they would pass to a regular CLI session so the wire
    /// payload (and therefore the prompt cache key) matches across modes.
    #[must_use]
    pub fn with_tools(mut self, tools: Vec<ToolDefinition>) -> Self {
        self.tools = tools;
        self
    }
}

/// Pure helper that translates a runtime [`ApiRequest`] into the wire-level
/// [`MessageRequest`]. Extracted from [`RpcApiClient::stream`] so the
/// transformation is unit-testable without spinning up a tokio runtime or
/// HTTP client.
pub(crate) fn build_rpc_message_request(
    model: &str,
    tools: &[ToolDefinition],
    request: ApiRequest,
) -> Result<MessageRequest, RuntimeError> {
    let tools = if tools.is_empty() {
        None
    } else {
        Some(tools.to_vec())
    };
    let tool_choice = tools.as_ref().map(|_| ToolChoice::Auto);
    Ok(MessageRequest {
        model: model.to_string(),
        max_tokens: 4096,
        messages: request
            .messages
            .into_iter()
            .map(conversation_message_to_input_message)
            .collect::<Result<Vec<_>, _>>()?,
        system: if request.system_prompt.is_empty() {
            None
        } else {
            Some(request.system_prompt.join("\n\n"))
        },
        tools,
        tool_choice,
        stream: false,
        ..Default::default()
    })
}

impl ApiClient for RpcApiClient {
    fn stream(&mut self, request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
        let message_request = build_rpc_message_request(&self.model, &self.tools, request)?;

        let response = self
            .runtime
            .block_on(self.client.send_message(&message_request))
            .map_err(|e| RuntimeError::new(e.to_string()))?;

        let mut events = response_to_events(response);
        push_prompt_cache_record(&self.client, &mut events);
        Ok(events)
    }
}

fn conversation_message_to_input_message(
    msg: ConversationMessage,
) -> Result<ninmu_api::InputMessage, RuntimeError> {
    let role = match msg.role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "tool",
    }
    .to_string();

    let content = msg
        .blocks
        .into_iter()
        .map(|b| match b {
            ContentBlock::Text { text } => Ok(ninmu_api::InputContentBlock::Text { text }),
            ContentBlock::ToolUse { id, name, input } => {
                let value = serde_json::from_str(&input)
                    .map_err(|e| RuntimeError::new(format!("invalid tool input JSON: {e}")))?;
                Ok(ninmu_api::InputContentBlock::ToolUse {
                    id,
                    name,
                    input: value,
                })
            }
            ContentBlock::ToolResult {
                tool_use_id,
                output,
                is_error,
                ..
            } => Ok(ninmu_api::InputContentBlock::ToolResult {
                tool_use_id,
                content: vec![ToolResultContentBlock::Text { text: output }],
                is_error,
            }),
            ContentBlock::Thinking { thinking } => {
                Ok(ninmu_api::InputContentBlock::Thinking { thinking })
            }
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(ninmu_api::InputMessage { role, content })
}

fn response_to_events(response: MessageResponse) -> Vec<AssistantEvent> {
    let mut events = Vec::new();
    let mut pending_tool: Option<(String, String, String)> = None;

    for block in response.content {
        match block {
            OutputContentBlock::Text { text } => {
                events.push(AssistantEvent::TextDelta(text));
            }
            OutputContentBlock::ToolUse { id, name, input } => {
                if let Some((tid, tname, tinput)) = pending_tool.take() {
                    events.push(AssistantEvent::ToolUse {
                        id: tid,
                        name: tname,
                        input: tinput,
                    });
                }
                let input_str = serde_json::to_string(&input).unwrap_or_default();
                pending_tool = Some((id, name, input_str));
            }
            OutputContentBlock::Thinking { thinking, .. } => {
                events.push(AssistantEvent::ThinkingDelta(thinking));
            }
            OutputContentBlock::RedactedThinking { .. } => {}
        }
    }

    if let Some((id, name, input)) = pending_tool.take() {
        events.push(AssistantEvent::ToolUse { id, name, input });
    }

    events.push(AssistantEvent::Usage(usage_to_token_usage(&response.usage)));
    events.push(AssistantEvent::MessageStop);
    events
}

fn usage_to_token_usage(usage: &Usage) -> TokenUsage {
    TokenUsage {
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        cache_creation_input_tokens: usage.cache_creation_input_tokens,
        cache_read_input_tokens: usage.cache_read_input_tokens,
    }
}

fn push_prompt_cache_record(client: &ProviderClient, events: &mut Vec<AssistantEvent>) {
    if let Some(record) = client.take_last_prompt_cache_record() {
        if let Some(event) = prompt_cache_record_to_runtime_event(record) {
            events.push(AssistantEvent::PromptCache(event));
        }
    }
}

fn prompt_cache_record_to_runtime_event(
    record: ninmu_api::PromptCacheRecord,
) -> Option<ninmu_runtime::PromptCacheEvent> {
    let cache_break = record.cache_break?;
    Some(ninmu_runtime::PromptCacheEvent {
        unexpected: cache_break.unexpected,
        reason: cache_break.reason,
        previous_cache_read_input_tokens: cache_break.previous_cache_read_input_tokens,
        current_cache_read_input_tokens: cache_break.current_cache_read_input_tokens,
        token_drop: cache_break.token_drop,
    })
}

#[cfg(test)]
mod tests {
    use super::build_rpc_message_request;
    use ninmu_api::{ToolChoice, ToolDefinition};
    use ninmu_runtime::{ApiRequest, ConversationMessage, MessageRole};
    use serde_json::json;

    fn user_message(text: &str) -> ConversationMessage {
        ConversationMessage {
            role: MessageRole::User,
            blocks: vec![ninmu_runtime::ContentBlock::Text {
                text: text.to_string(),
            }],
            usage: None,
        }
    }

    #[test]
    fn build_rpc_message_request_omits_tools_when_empty() {
        let req = ApiRequest {
            messages: vec![user_message("hello")],
            system_prompt: vec!["You are helpful".to_string()],
        };
        let out = build_rpc_message_request("claude-sonnet-4-6", &[], req).unwrap();
        assert_eq!(out.tools, None, "no tools = no tool block in wire payload");
        assert_eq!(out.tool_choice, None);
    }

    #[test]
    fn build_rpc_message_request_passes_tools_through() {
        let tools = vec![
            ToolDefinition {
                name: "grep_search".to_string(),
                description: Some("search".to_string()),
                input_schema: json!({"type": "object"}),
            },
            ToolDefinition {
                name: "read_file".to_string(),
                description: Some("read".to_string()),
                input_schema: json!({"type": "object"}),
            },
        ];
        let req = ApiRequest {
            messages: vec![user_message("hi")],
            system_prompt: vec!["sys".to_string()],
        };
        let out = build_rpc_message_request("claude-sonnet-4-6", &tools, req).unwrap();
        assert_eq!(out.tools.as_deref(), Some(tools.as_slice()));
        assert!(matches!(out.tool_choice, Some(ToolChoice::Auto)));
    }

    #[test]
    fn build_rpc_message_request_joins_system_sections() {
        let req = ApiRequest {
            messages: vec![user_message("x")],
            system_prompt: vec!["one".to_string(), "two".to_string()],
        };
        let out = build_rpc_message_request("model", &[], req).unwrap();
        assert_eq!(out.system.as_deref(), Some("one\n\ntwo"));
    }

    #[test]
    fn build_rpc_message_request_no_system_when_empty() {
        let req = ApiRequest {
            messages: vec![user_message("x")],
            system_prompt: vec![],
        };
        let out = build_rpc_message_request("model", &[], req).unwrap();
        assert_eq!(out.system, None);
    }
}

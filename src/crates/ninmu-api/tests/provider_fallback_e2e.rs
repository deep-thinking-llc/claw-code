#![allow(clippy::doc_markdown, clippy::uninlined_format_args, unused_imports)]
use std::sync::Arc;
use std::time::Duration;

use ninmu_api::{
    detect_provider_kind, InputContentBlock, InputMessage, MessageRequest, OpenAiCompatClient,
    OpenAiCompatConfig, OutputContentBlock, ProviderClient, ProviderKind,
};
use serde_json::json;
use tokio::sync::Mutex;

mod common;
use common::*;

fn openai_response(model: &str, text: &str) -> String {
    let body = json!({
        "id": "chatcmpl-test",
        "object": "chat.completion",
        "model": model,
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": text},
            "finish_reason": "stop"
        }],
        "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
    });
    http_response("200 OK", "application/json", &body.to_string())
}

fn error_response(status: &str, code: u16) -> String {
    let body = json!({"error": {"message": "server error", "type": "server_error", "code": code}});
    http_response(status, "application/json", &body.to_string())
}

fn sample_request(model: &str) -> MessageRequest {
    MessageRequest {
        model: model.to_string(),
        max_tokens: 64,
        messages: vec![InputMessage {
            role: "user".to_string(),
            content: vec![InputContentBlock::Text {
                text: "hello".to_string(),
            }],
        }],
        stream: false,
        ..Default::default()
    }
}

fn test_client(base_url: &str) -> OpenAiCompatClient {
    OpenAiCompatClient::new("test-key", OpenAiCompatConfig::openai())
        .with_base_url(base_url)
        .with_retry_policy(0, Duration::from_millis(1), Duration::from_millis(1))
}

#[tokio::test]
async fn fallback_on_primary_500_error() {
    let primary_state = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
    let fallback_state = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
    let primary = spawn_server(
        primary_state.clone(),
        vec![error_response("500 Internal Server Error", 500)],
        false,
    )
    .await;
    let fallback = spawn_server(
        fallback_state.clone(),
        vec![openai_response("gpt-4o", "fallback response")],
        false,
    )
    .await;

    let primary_result = test_client(&primary.base_url())
        .send_message(&sample_request("gpt-4o"))
        .await;
    assert!(primary_result.is_err(), "primary should fail");

    let fallback_result = test_client(&fallback.base_url())
        .send_message(&sample_request("gpt-4o"))
        .await;
    assert!(
        fallback_result.is_ok(),
        "fallback should succeed: {:?}",
        fallback_result
    );
    let response = fallback_result.unwrap();
    assert_eq!(
        response.content[0],
        OutputContentBlock::Text {
            text: "fallback response".to_string(),
        }
    );

    assert_eq!(primary_state.lock().await.len(), 1);
    assert_eq!(fallback_state.lock().await.len(), 1);
}

#[tokio::test]
async fn no_fallback_on_primary_success() {
    let primary_state = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
    let fallback_state = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
    let primary = spawn_server(
        primary_state.clone(),
        vec![openai_response("gpt-4o", "primary ok")],
        false,
    )
    .await;
    let _fallback = spawn_server(
        fallback_state.clone(),
        vec![openai_response("gpt-4o", "fallback not used")],
        false,
    )
    .await;

    let result = test_client(&primary.base_url())
        .send_message(&sample_request("gpt-4o"))
        .await;
    assert!(result.is_ok(), "primary should succeed: {:?}", result);
    assert_eq!(
        result.unwrap().content[0],
        OutputContentBlock::Text {
            text: "primary ok".to_string(),
        }
    );

    assert_eq!(primary_state.lock().await.len(), 1);
    assert_eq!(
        fallback_state.lock().await.len(),
        0,
        "fallback should not be called"
    );
}

#[tokio::test]
async fn fallback_on_primary_rate_limit() {
    let primary_state = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
    let fallback_state = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
    let primary = spawn_server(
        primary_state.clone(),
        vec![error_response("429 Too Many Requests", 429)],
        false,
    )
    .await;
    let fallback = spawn_server(
        fallback_state.clone(),
        vec![openai_response("claude-sonnet-4-6", "rate-limit fallback")],
        false,
    )
    .await;

    let primary_result = test_client(&primary.base_url())
        .send_message(&sample_request("gpt-4o"))
        .await;
    assert!(primary_result.is_err(), "primary should fail with 429");

    let fallback_result = test_client(&fallback.base_url())
        .send_message(&sample_request("claude-sonnet-4-6"))
        .await;
    assert!(
        fallback_result.is_ok(),
        "fallback should succeed: {:?}",
        fallback_result
    );
}

#[tokio::test]
async fn error_propagates_when_all_providers_fail() {
    let primary_state = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
    let primary = spawn_server(
        primary_state.clone(),
        vec![error_response("500 Internal Server Error", 500)],
        false,
    )
    .await;

    let result = test_client(&primary.base_url())
        .send_message(&sample_request("gpt-4o"))
        .await;
    assert!(result.is_err(), "should fail: {:?}", result);
}

#[tokio::test]
async fn fallback_preserves_model_name() {
    let fallback_state = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
    let fallback = spawn_server(
        fallback_state.clone(),
        vec![openai_response("claude-sonnet-4-6", "model check")],
        false,
    )
    .await;

    let result = test_client(&fallback.base_url())
        .send_message(&sample_request("claude-sonnet-4-6"))
        .await;
    assert!(result.is_ok(), "should succeed: {:?}", result);

    let captured = fallback_state.lock().await;
    let body: serde_json::Value = serde_json::from_str(&captured[0].body).unwrap();
    assert_eq!(
        body["model"], "claude-sonnet-4-6",
        "model name should be preserved"
    );
}

#[tokio::test]
async fn provider_client_routes_openai_from_env() {
    let _lock = env_lock();
    let _key = EnvVarGuard::set("OPENAI_API_KEY", Some("test-key"));
    let _base = EnvVarGuard::set("OPENAI_BASE_URL", None);
    let _anthropic = EnvVarGuard::set("ANTHROPIC_API_KEY", None);

    let kind = detect_provider_kind("gpt-4o");
    assert_eq!(kind, ProviderKind::OpenAi);
}

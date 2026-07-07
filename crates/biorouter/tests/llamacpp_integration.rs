//! Live integration tests for the Llama Server provider + managed sidecar.
//!
//! These exercise a real llama-server process with a real (tiny) model, so
//! they are `#[ignore]`d by default. Run them explicitly:
//!
//! ```sh
//! BIOROUTER_LLAMACPP_BIN=ui/desktop/src/bin/llamacpp/llama-server \
//!   cargo test -p biorouter --test llamacpp_integration -- --ignored --test-threads=1
//! ```
//!
//! The first run downloads a tiny GGUF test model (~0.5 GB) from Hugging Face
//! into the Biorouter llama.cpp model cache; later runs reuse it.

use biorouter::conversation::message::{Message, MessageContent};
use biorouter::model::ModelConfig;
use biorouter::providers::base::Provider;
use biorouter::providers::llamacpp::LlamaCppProvider;
use futures::StreamExt;
use rmcp::{model::Tool, object};

const TEST_MODEL: &str = "unsloth/Qwen3.5-0.8B-GGUF:Q4_K_M";

fn weather_tool() -> Tool {
    Tool::new(
        "get_weather".to_string(),
        "Get current temperature for a given location.".to_string(),
        object!({
            "type": "object",
            "required": ["location"],
            "properties": {
                "location": {"type": "string"}
            }
        }),
    )
}

async fn provider() -> LlamaCppProvider {
    let model = ModelConfig::new(TEST_MODEL)
        .unwrap()
        .with_max_tokens(Some(200));
    LlamaCppProvider::from_env(model).await.unwrap()
}

#[tokio::test]
#[ignore = "spawns a real llama-server and downloads a model"]
async fn chat_completion_roundtrip() {
    let provider = provider().await;
    let messages = vec![Message::user().with_text("Reply with exactly the word: pong")];
    let (message, usage) = provider
        .complete("You are a terse assistant.", &messages, &[])
        .await
        .expect("completion should succeed");

    let text = message.as_concat_text();
    assert!(
        !text.is_empty(),
        "expected non-empty completion, got: {text:?}"
    );
    assert!(
        usage.usage.total_tokens.unwrap_or(0) > 0,
        "expected token usage to be reported"
    );
    println!("completion: {text}");
}

#[tokio::test]
#[ignore = "spawns a real llama-server and downloads a model"]
async fn streaming_completion_works() {
    let provider = provider().await;
    let messages = vec![Message::user().with_text("Count from 1 to 5, digits only.")];
    let mut stream = provider
        .stream("You are a terse assistant.", &messages, &[])
        .await
        .expect("stream should start");

    let mut chunks = 0;
    let mut text = String::new();
    while let Some(item) = stream.next().await {
        let (message, _usage) = item.expect("stream item should be ok");
        if let Some(message) = message {
            text.push_str(&message.as_concat_text());
        }
        chunks += 1;
    }
    assert!(
        chunks > 1,
        "expected multiple streamed chunks, got {chunks}"
    );
    assert!(!text.is_empty(), "expected streamed text");
    println!("streamed {chunks} chunks: {text}");
}

#[tokio::test]
#[ignore = "spawns a real llama-server and downloads a model"]
async fn tool_calling_emits_tool_request() {
    let provider = provider().await;
    let messages = vec![Message::user()
        .with_text("What is the weather in Paris? Use the get_weather tool to find out.")];
    let (message, _usage) = provider
        .complete(
            "You are an AI agent. Always use the provided tools to answer questions.",
            &messages,
            &[weather_tool()],
        )
        .await
        .expect("tool-call completion should succeed");

    let has_tool_request = message
        .content
        .iter()
        .any(|c| matches!(c, MessageContent::ToolRequest(_)));
    assert!(
        has_tool_request,
        "expected a get_weather tool call, got: {:?}",
        message.as_concat_text()
    );
}

#[tokio::test]
#[ignore = "spawns a real llama-server and downloads a model"]
async fn switching_models_restarts_sidecar() {
    use biorouter::providers::llamacpp::resolve_model_source;
    use biorouter::providers::llamacpp_sidecar::{global, SidecarState};
    use std::time::Duration;

    let source = resolve_model_source(TEST_MODEL).unwrap();
    let port_a = global().ensure(TEST_MODEL, &source).await.unwrap();
    global()
        .wait_ready(Duration::from_secs(1800))
        .await
        .unwrap();

    // Re-ensuring the same model must keep the same process/port.
    let port_b = global().ensure(TEST_MODEL, &source).await.unwrap();
    assert_eq!(port_a, port_b);

    let status = global().status().await;
    assert_eq!(status.state, SidecarState::Ready);
    assert_eq!(status.model.as_deref(), Some(TEST_MODEL));

    global().stop().await;
    let status = global().status().await;
    assert_eq!(status.state, SidecarState::Stopped);
}

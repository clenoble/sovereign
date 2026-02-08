//! Integration tests for LLM inference. Requires GPU + model files.
//! Run with: cargo test -p sovereign-ai -- --ignored

use sovereign_core::interfaces::ModelBackend;

/// Test loading and generating with the 3B router model.
/// Requires: models/qwen2.5-3b-instruct-q4_k_m.gguf
#[tokio::test]
#[ignore = "Requires GPU and model files"]
async fn load_and_generate_3b() {
    let mut backend = sovereign_ai::llm::AsyncLlmBackend::new(4096);

    backend
        .load("models/qwen2.5-3b-instruct-q4_k_m.gguf", 99)
        .await
        .expect("Failed to load 3B model");

    let prompt = sovereign_ai::llm::prompt::qwen_chat_prompt(
        "You are a helpful assistant.",
        "What is 2 + 2?",
    );

    let response = backend
        .generate(&prompt, 50)
        .await
        .expect("Failed to generate");

    assert!(!response.is_empty(), "Response should not be empty");
    println!("3B response: {response}");

    backend.unload().await.expect("Failed to unload");
}

/// Test that generating without loading a model fails cleanly.
#[tokio::test]
async fn generate_without_load_fails() {
    let backend = sovereign_ai::llm::AsyncLlmBackend::new(4096);

    let result = backend.generate("hello", 10).await;
    assert!(result.is_err(), "Should fail with 'Model not loaded'");
}

/// Test that unloading without loading succeeds (no-op).
#[tokio::test]
async fn unload_without_load_succeeds() {
    let mut backend = sovereign_ai::llm::AsyncLlmBackend::new(4096);

    // Unload when nothing is loaded — should succeed (drops None)
    let result = backend.unload().await;
    assert!(result.is_ok(), "Unloading empty backend should succeed");
}

/// Test loading a nonexistent model fails cleanly.
#[tokio::test]
async fn load_nonexistent_model_fails() {
    let mut backend = sovereign_ai::llm::AsyncLlmBackend::new(4096);

    let result = backend.load("/nonexistent/model.gguf", 99).await;
    assert!(result.is_err(), "Should fail for nonexistent model path");
}

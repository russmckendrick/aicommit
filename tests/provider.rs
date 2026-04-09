use aicommit::{
    ai::{AiEngine, ChatMessage, engine_from_config, openai_compat::OpenAiCompatEngine},
    config::Config,
};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path},
};

#[tokio::test]
async fn openai_compatible_engine_reads_chat_response() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("authorization", "Bearer key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [
                { "message": { "content": "<think>hidden</think>\nfeat: add cli" } }
            ]
        })))
        .mount(&server)
        .await;

    let config = Config {
        api_key: Some("key".to_owned()),
        api_url: Some(format!("{}/v1", server.uri())),
        ..Config::default()
    };
    let engine = OpenAiCompatEngine::new(config).unwrap();
    let response = engine
        .generate_commit_message(&[ChatMessage::user("diff")])
        .await
        .unwrap();

    assert_eq!(response, "feat: add cli");
}

#[tokio::test]
async fn azure_openai_engine_uses_api_key_header() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/openai/v1/chat/completions"))
        .and(header("api-key", "key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [
                { "message": { "content": "feat: add azure openai" } }
            ]
        })))
        .mount(&server)
        .await;

    let config = Config {
        ai_provider: "azure-openai".to_owned(),
        api_key: Some("key".to_owned()),
        api_url: Some(format!("{}/openai/v1", server.uri())),
        ..Config::default()
    };
    let engine = OpenAiCompatEngine::new(config).unwrap();
    let response = engine
        .generate_commit_message(&[ChatMessage::user("diff")])
        .await
        .unwrap();

    assert_eq!(response, "feat: add azure openai");
}

#[test]
fn engine_from_config_accepts_local_cli_providers() {
    let claude = Config {
        ai_provider: "claude-code".to_owned(),
        model: "default".to_owned(),
        ..Config::default()
    };
    let codex = Config {
        ai_provider: "codex".to_owned(),
        model: "default".to_owned(),
        ..Config::default()
    };

    assert!(engine_from_config(&claude).is_ok());
    assert!(engine_from_config(&codex).is_ok());
}

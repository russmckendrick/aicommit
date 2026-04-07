use aicommit::{
    ai::{AiEngine, ChatMessage, openai_compat::OpenAiCompatEngine},
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

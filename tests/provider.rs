use aicommit::{
    ai::{AiEngine, ChatMessage, engine_from_config, openai_compat::OpenAiCompatEngine},
    config::Config,
    generator,
    git::CommitInfo,
    prompt::build_pr_messages,
    token::{count_messages, count_tokens},
};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_string_contains, header, method, path},
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

#[tokio::test]
async fn generate_pull_request_synthesizes_chunked_diff() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_string_contains("This is diff chunk"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [
                { "message": { "content": "- Capture one slice of the PR diff" } }
            ]
        })))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_string_contains("Partial summaries from cumulative PR diff"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [
                { "message": { "content": "feat(cli): generate PR drafts\n\n## Summary\n- Combine chunk summaries into one PR draft\n\n## Testing\n- cargo test" } }
            ]
        })))
        .mount(&server)
        .await;

    let config = Config {
        api_key: Some("key".to_owned()),
        api_url: Some(format!("{}/v1", server.uri())),
        tokens_max_input: 500,
        tokens_max_output: 80,
        ..Config::default()
    };
    let commits = vec![CommitInfo {
        hash: "abc123".to_owned(),
        subject: "feat(cli): add PR command".to_owned(),
        body: String::new(),
    }];
    let files = vec!["src/cli.rs".to_owned()];
    let prompt_tokens = count_messages(
        &build_pr_messages(
            &config,
            "",
            "",
            "main",
            Some("feature/pr"),
            None,
            &commits,
            &files,
        )
        .unwrap(),
    );
    let available = config
        .tokens_max_input
        .saturating_sub(config.tokens_max_output)
        .saturating_sub(prompt_tokens)
        .saturating_sub(20)
        .max(1);
    let mut diff = "diff --git a/src/cli.rs b/src/cli.rs\n".to_owned();
    while count_tokens(&diff) <= available {
        diff.push_str("@@\n+new line in chunked diff\n");
    }

    let draft = generator::generate_pull_request(
        &config,
        &diff,
        "",
        "main",
        Some("feature/pr"),
        None,
        &commits,
        &files,
    )
    .await
    .unwrap();

    assert_eq!(draft.title, "feat(cli): generate PR drafts");
    assert!(draft.body.contains("## Summary"));
    assert!(draft.body.contains("## Testing"));
}

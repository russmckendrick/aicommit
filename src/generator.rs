use anyhow::Result;

use crate::{
    ai::engine_from_config,
    config::Config,
    prompt::{build_messages, initial_messages},
    token::{count_messages, split_diff},
};

const TOKEN_ADJUSTMENT: usize = 20;

pub async fn generate_commit_message(
    config: &Config,
    diff: &str,
    full_gitmoji_spec: bool,
    context: &str,
) -> Result<String> {
    let prompt_tokens = count_messages(&initial_messages(config, full_gitmoji_spec, context));
    let max_request_tokens = config
        .tokens_max_input
        .saturating_sub(config.tokens_max_output)
        .saturating_sub(prompt_tokens)
        .saturating_sub(TOKEN_ADJUSTMENT);

    let chunks = split_diff(diff, max_request_tokens.max(1))?;
    let engine = engine_from_config(config)?;
    let mut messages = Vec::with_capacity(chunks.len());

    for chunk in chunks {
        let chat_messages = build_messages(config, &chunk, full_gitmoji_spec, context);
        messages.push(engine.generate_commit_message(&chat_messages).await?);
    }

    Ok(messages.join("\n\n"))
}

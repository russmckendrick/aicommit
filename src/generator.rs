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
    let prompt_tokens = count_messages(&initial_messages(config, full_gitmoji_spec, context)?);
    let max_request_tokens = config
        .tokens_max_input
        .saturating_sub(config.tokens_max_output)
        .saturating_sub(prompt_tokens)
        .saturating_sub(TOKEN_ADJUSTMENT);

    let chunks = split_diff(diff, max_request_tokens.max(1))?;
    let engine = engine_from_config(config)?;

    if chunks.len() == 1 {
        let chat_messages = build_messages(config, &chunks[0], full_gitmoji_spec, context)?;
        return engine.generate_commit_message(&chat_messages).await;
    }

    let mut summaries = Vec::with_capacity(chunks.len());
    for (index, chunk) in chunks.iter().enumerate() {
        let chunk_context = format!(
            "{context}\nThis is diff chunk {} of {}. Summarize the change intent in one short phrase for later synthesis. Do not write a final commit message.",
            index + 1,
            chunks.len()
        );
        let chat_messages = build_messages(config, chunk, full_gitmoji_spec, &chunk_context)?;
        summaries.push(engine.generate_commit_message(&chat_messages).await?);
    }

    let synthesis_input = format!(
        "Partial summaries from a large staged diff:\n{}\n\nSynthesize these into exactly one final commit message.",
        summaries
            .iter()
            .enumerate()
            .map(|(index, summary)| format!("{}. {}", index + 1, summary))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let synthesis_context = format!(
        "{context}\nYou are synthesizing partial summaries from one staged diff. Return exactly one final commit message, not one line per summary."
    );
    let chat_messages = build_messages(
        config,
        &synthesis_input,
        full_gitmoji_spec,
        &synthesis_context,
    )?;
    engine.generate_commit_message(&chat_messages).await
}

use anyhow::{Result, bail};

use crate::{
    ai::engine_from_config,
    config::Config,
    errors::AicError,
    git,
    prompt::{build_review_messages, review_system_prompt},
    token::{count_messages, count_tokens, split_diff},
    ui,
};

const TOKEN_ADJUSTMENT: usize = 20;

pub async fn run(context: String) -> Result<()> {
    git::assert_git_repo()?;
    let config = Config::load()?;

    if config.provider_needs_api_key() && config.api_key.is_none() {
        bail!(AicError::MissingApiKey(config.ai_provider));
    }

    super::commit::ensure_staged_files().await?;
    let staged = git::staged_files()?;
    if staged.is_empty() {
        bail!(AicError::NoChanges);
    }

    let diff = git::staged_diff(&staged)?;
    if diff.trim().is_empty() {
        bail!("no diff content available after applying ignore and binary filters");
    }

    ui::section(format!("Reviewing staged files ({})", staged.len()));
    for file in &staged {
        ui::bullet(file);
    }

    let spinner = ui::spinner("Analyzing changes");
    let review = generate_review(&config, &diff, &context).await;
    spinner.finish_and_clear();

    let review = review?;
    ui::blank_line();
    ui::section("Review");
    println!("{review}");

    Ok(())
}

async fn generate_review(config: &Config, diff: &str, context: &str) -> Result<String> {
    let system_prompt = review_system_prompt(config, context)?;
    let system_tokens = count_messages(&[crate::ai::ChatMessage::system(&system_prompt)]);
    let max_request_tokens = config
        .tokens_max_input
        .saturating_sub(config.tokens_max_output)
        .saturating_sub(system_tokens)
        .saturating_sub(TOKEN_ADJUSTMENT);

    let chunks = split_diff(diff, max_request_tokens.max(1))?;
    let engine = engine_from_config(config)?;

    if chunks.len() == 1 {
        let messages = build_review_messages(config, &chunks[0], context)?;
        return engine.generate_commit_message(&messages).await;
    }

    let mut partial_reviews = Vec::with_capacity(chunks.len());
    for (i, chunk) in chunks.iter().enumerate() {
        let chunk_context = format!(
            "{context}\nThis is diff chunk {} of {}. List findings for this chunk only.",
            i + 1,
            chunks.len()
        );
        let messages = build_review_messages(config, chunk, &chunk_context)?;
        partial_reviews.push(engine.generate_commit_message(&messages).await?);
    }

    let per_chunk_budget = max_request_tokens / partial_reviews.len().max(1);
    let synthesis_parts: Vec<String> = partial_reviews
        .iter()
        .enumerate()
        .map(|(i, review)| {
            let header = format!("--- Chunk {} ---\n", i + 1);
            let available = per_chunk_budget.saturating_sub(count_tokens(&header));
            let lines: Vec<&str> = review.lines().collect();
            let mut truncated = String::new();
            for line in &lines {
                if count_tokens(&truncated) + count_tokens(line) > available {
                    truncated.push_str("[...truncated]\n");
                    break;
                }
                truncated.push_str(line);
                truncated.push('\n');
            }
            format!("{header}{truncated}")
        })
        .collect();

    let synthesis_input = format!(
        "Partial review findings from a large staged diff:\n{}\n\nSynthesize into one unified review. Deduplicate, prioritize by severity, and present a cohesive summary.",
        synthesis_parts.join("\n\n")
    );
    let messages = build_review_messages(config, &synthesis_input, context)?;
    engine.generate_commit_message(&messages).await
}

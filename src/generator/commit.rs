use anyhow::Result;

use crate::{
    ai::{AiEngine, ChatMessage, DeltaFn, engine_from_config},
    config::{Config, provider_uses_compact_prompts},
    prompt::{
        build_messages, compact_diff_for_small_model, initial_messages, tidy_small_model_commit,
    },
    token::{count_messages, split_diff},
};

use super::{GenerationProgress, ProgressFn, report};

const TOKEN_ADJUSTMENT: usize = 20;

fn chunk_summary_context(context: &str, current: usize, total: usize) -> String {
    format!(
        "{context}\nThis is diff chunk {current} of {total}. Summarize the change intent in one short phrase for later synthesis. Do not write a final commit message."
    )
}

pub async fn generate_commit_message(
    config: &Config,
    diff: &str,
    full_gitmoji_spec: bool,
    context: &str,
    staged_files: &[String],
    progress: Option<ProgressFn<'_>>,
) -> Result<String> {
    generate_commit_message_streaming(
        config,
        diff,
        full_gitmoji_spec,
        context,
        staged_files,
        progress,
        None,
    )
    .await
}

/// Generate a commit message, streaming the final request's text to
/// `on_delta` as it arrives. Chunk-summary requests never stream: only the
/// call that produces the message the user will see does.
pub async fn generate_commit_message_streaming(
    config: &Config,
    diff: &str,
    full_gitmoji_spec: bool,
    context: &str,
    staged_files: &[String],
    progress: Option<ProgressFn<'_>>,
    on_delta: Option<DeltaFn<'_>>,
) -> Result<String> {
    // Budget against the chunk-summary prompt variant - the largest context
    // this function sends. Measuring the bare context under-reserves and can
    // trip the engine's token guard on tightly-packed chunks.
    let prompt_tokens = count_messages(&initial_messages(
        config,
        full_gitmoji_spec,
        &chunk_summary_context(context, 9999, 9999),
        staged_files,
    )?);
    let max_request_tokens = config
        .tokens_max_input
        .saturating_sub(config.tokens_max_output)
        .saturating_sub(prompt_tokens)
        .saturating_sub(TOKEN_ADJUSTMENT);

    let compact = provider_uses_compact_prompts(&config.ai_provider);
    let compacted;
    let diff = if compact {
        compacted = compact_diff_for_small_model(diff);
        compacted.as_str()
    } else {
        diff
    };

    report(progress, GenerationProgress::Splitting);
    let chunks = split_diff(diff, max_request_tokens.max(1))?;
    let engine = engine_from_config(config)?;

    if chunks.len() == 1 {
        report(
            progress,
            GenerationProgress::Chunk {
                current: 1,
                total: 1,
            },
        );
        let chat_messages =
            build_messages(config, &chunks[0], full_gitmoji_spec, context, staged_files)?;
        let message = final_call(engine.as_ref(), &chat_messages, on_delta).await?;
        return Ok(finish_message(config, full_gitmoji_spec, message));
    }

    let mut summaries = Vec::with_capacity(chunks.len());
    for (index, chunk) in chunks.iter().enumerate() {
        report(
            progress,
            GenerationProgress::Chunk {
                current: index + 1,
                total: chunks.len(),
            },
        );
        let chunk_context = chunk_summary_context(context, index + 1, chunks.len());
        let chat_messages = build_messages(
            config,
            chunk,
            full_gitmoji_spec,
            &chunk_context,
            staged_files,
        )?;
        summaries.push(engine.complete(&chat_messages).await?);
    }

    report(progress, GenerationProgress::Synthesizing);
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
        staged_files,
    )?;
    let message = final_call(engine.as_ref(), &chat_messages, on_delta).await?;
    Ok(finish_message(config, full_gitmoji_spec, message))
}

async fn final_call(
    engine: &dyn AiEngine,
    messages: &[ChatMessage],
    on_delta: Option<DeltaFn<'_>>,
) -> Result<String> {
    match on_delta {
        Some(on_delta) => engine.complete_streaming(messages, on_delta).await,
        None => engine.complete(messages).await,
    }
}

fn finish_message(config: &Config, full_gitmoji_spec: bool, message: String) -> String {
    if provider_uses_compact_prompts(&config.ai_provider) {
        tidy_small_model_commit(&message, config.emoji && !full_gitmoji_spec)
    } else {
        message
    }
}

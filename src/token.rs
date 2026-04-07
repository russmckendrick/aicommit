use anyhow::Result;
use tiktoken_rs::cl100k_base;

use crate::{ai::ChatMessage, errors::AicError};

pub fn count_tokens(input: &str) -> usize {
    match cl100k_base() {
        Ok(encoder) => encoder.encode_with_special_tokens(input).len(),
        Err(_) => input.split_whitespace().count(),
    }
}

pub fn count_messages(messages: &[ChatMessage]) -> usize {
    messages
        .iter()
        .map(|message| count_tokens(&message.content) + 4)
        .sum()
}

pub fn split_diff(diff: &str, max_tokens: usize) -> Result<Vec<String>> {
    if count_tokens(diff) <= max_tokens {
        return Ok(vec![diff.to_owned()]);
    }

    let mut chunks = Vec::new();
    let mut current = String::new();

    for line in diff.lines() {
        let proposed = if current.is_empty() {
            line.to_owned()
        } else {
            format!("{current}\n{line}")
        };

        if count_tokens(&proposed) > max_tokens {
            if current.is_empty() {
                return Err(AicError::TooManyTokens.into());
            }
            chunks.push(current);
            current = line.to_owned();
        } else {
            current = proposed;
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    Ok(chunks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_diff_keeps_small_diff_whole() {
        let chunks = split_diff("one\ntwo", 100).unwrap();
        assert_eq!(chunks, vec!["one\ntwo"]);
    }

    #[test]
    fn counts_messages_with_overhead() {
        let messages = vec![ChatMessage::user("hello")];
        assert!(count_messages(&messages) >= 5);
    }
}

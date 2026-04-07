use thiserror::Error;

#[derive(Debug, Error)]
pub enum AicError {
    #[error("unsupported config key: {0}")]
    UnsupportedConfigKey(String),

    #[error("invalid value for {key}: {message}")]
    InvalidConfigValue { key: String, message: String },

    #[error("no changes detected")]
    NoChanges,

    #[error("not a git repository")]
    NotGitRepository,

    #[error("API key is missing for provider '{0}'")]
    MissingApiKey(String),

    #[error("model '{model}' is not available for provider '{provider}'")]
    ModelNotFound { provider: String, model: String },

    #[error("authentication failed for provider '{0}'")]
    Authentication(String),

    #[error("rate limit exceeded for provider '{0}'")]
    RateLimited(String),

    #[error("insufficient credits or quota for provider '{0}'")]
    InsufficientCredits(String),

    #[error("service unavailable for provider '{0}'")]
    ServiceUnavailable(String),

    #[error("AI provider returned an empty commit message")]
    EmptyMessage,

    #[error("diff is too large for the configured token limits")]
    TooManyTokens,
}

pub fn normalize_provider_error(
    provider: &str,
    model: &str,
    status: Option<u16>,
    body: &str,
) -> AicError {
    let lower = body.to_lowercase();

    match status {
        Some(401) => return AicError::Authentication(provider.to_owned()),
        Some(402) => return AicError::InsufficientCredits(provider.to_owned()),
        Some(404) if mentions_model_problem(&lower) => {
            return AicError::ModelNotFound {
                provider: provider.to_owned(),
                model: model.to_owned(),
            };
        }
        Some(429) => return AicError::RateLimited(provider.to_owned()),
        Some(500..=599) => return AicError::ServiceUnavailable(provider.to_owned()),
        _ => {}
    }

    if mentions_model_problem(&lower) {
        AicError::ModelNotFound {
            provider: provider.to_owned(),
            model: model.to_owned(),
        }
    } else if lower.contains("api key")
        || lower.contains("apikey")
        || lower.contains("authentication")
        || lower.contains("unauthorized")
    {
        AicError::Authentication(provider.to_owned())
    } else if lower.contains("rate limit") || lower.contains("too many requests") {
        AicError::RateLimited(provider.to_owned())
    } else if lower.contains("credit")
        || lower.contains("quota")
        || lower.contains("billing")
        || lower.contains("payment")
    {
        AicError::InsufficientCredits(provider.to_owned())
    } else {
        AicError::ServiceUnavailable(format!("{provider}: {body}"))
    }
}

fn mentions_model_problem(lower: &str) -> bool {
    lower.contains("model")
        && (lower.contains("not found")
            || lower.contains("does not exist")
            || lower.contains("invalid")
            || lower.contains("pull"))
}

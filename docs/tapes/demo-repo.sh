#!/bin/sh
# Create a throwaway repository with a mixed staged change set for the
# terminal UI demo tapes. Prints the repository path.
set -e
repo=$(mktemp -d "${TMPDIR:-/tmp}/aic-demo.XXXXXX")
cd "$repo"
git init -q -b main
git config user.email demo@example.com
git config user.name "aic demo"

mkdir -p src docs
cat > src/engine.rs <<'RS'
pub trait Engine {
    async fn generate_commit_message(&self, prompt: &str) -> String;
}

pub fn label(stage: &str) -> &str {
    match stage {
        "split" => "Splitting diff",
        "synth" => "Synthesizing",
        _ => "Working",
    }
}
RS
printf '# Providers\n\n| Provider | Notes |\n|---|---|\n| openai | HTTP |\n' > docs/providers.md
git add -A
git commit -q -m "feat: initial engine"

cat > src/engine.rs <<'RS'
pub trait Engine {
    async fn complete(&self, prompt: &str) -> String;

    /// Stream fragments as they arrive; defaults to one fragment.
    async fn complete_streaming(&self, prompt: &str, on_delta: &dyn Fn(&str)) -> String {
        let text = self.complete(prompt).await;
        on_delta(&text);
        text
    }
}

pub fn label(stage: &str) -> &str {
    match stage {
        "split" => "Preparing diff",
        "synth" => "Synthesising",
        _ => "Working",
    }
}
RS
printf '# Providers\n\n| Provider | Streams |\n|---|---|\n| openai | Yes |\n| anthropic | Yes |\n' > docs/providers.md
git add -A
echo "$repo"

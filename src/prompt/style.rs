//! Infer a repository's commit-message habits from recent subjects, so
//! generated messages match the history they join: scopes already in use,
//! description casing, trailing full stops, and typical subject length.
//!
//! The emoji and conventional-type choices stay with `AIC_EMOJI` and the
//! prompt's convention section; this only adds hints that cannot conflict
//! with them.

use std::collections::BTreeMap;

const MIN_SUBJECTS: usize = 8;
const MAX_SCOPES: usize = 8;

struct Parsed<'a> {
    scope: Option<&'a str>,
    description: &'a str,
}

/// Strip a leading GitMoji (`✨ ` or `:sparkles: `) and split
/// `type(scope)!: description`.
fn parse_subject(subject: &str) -> Option<Parsed<'_>> {
    let mut rest = subject.trim();
    if rest.starts_with(':')
        && let Some(end) = rest[1..].find(':')
        && rest[1..=end]
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        rest = rest[end + 2..].trim_start();
    }
    while rest.chars().next().is_some_and(|c| !c.is_ascii()) {
        let mut chars = rest.chars();
        chars.next();
        rest = chars.as_str();
    }
    rest = rest.trim_start();

    let (head, description) = rest.split_once(": ")?;
    let head = head.trim_end_matches('!');
    let (kind, scope) = match head.split_once('(') {
        Some((kind, scope)) => (kind, Some(scope.strip_suffix(')')?)),
        None => (head, None),
    };
    if kind.is_empty() || !kind.chars().all(|c| c.is_ascii_lowercase()) {
        return None;
    }
    Some(Parsed {
        scope: scope.filter(|scope| !scope.is_empty()),
        description,
    })
}

/// A short "match this repository" instruction, or `None` when there is
/// too little history to say anything useful.
pub fn infer_repo_style(subjects: &[String]) -> Option<String> {
    let subjects = subjects
        .iter()
        .map(|subject| subject.trim())
        .filter(|subject| !subject.is_empty() && !subject.starts_with("Merge "))
        .collect::<Vec<_>>();
    if subjects.len() < MIN_SUBJECTS {
        return None;
    }

    let parsed = subjects
        .iter()
        .filter_map(|subject| parse_subject(subject))
        .collect::<Vec<_>>();
    let mut lines = Vec::new();

    let mut scopes: BTreeMap<&str, usize> = BTreeMap::new();
    for item in &parsed {
        if let Some(scope) = item.scope {
            *scopes.entry(scope).or_insert(0) += 1;
        }
    }
    let mut ranked = scopes
        .into_iter()
        .filter(|(_, count)| *count >= 2)
        .collect::<Vec<_>>();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    if !ranked.is_empty() {
        let names = ranked
            .iter()
            .take(MAX_SCOPES)
            .map(|(scope, _)| *scope)
            .collect::<Vec<_>>();
        lines.push(format!(
            "- Scopes already used here: {}. Prefer one of these over inventing a new scope.",
            names.join(", ")
        ));
    }

    let descriptions = if parsed.len() * 2 >= subjects.len() {
        parsed
            .iter()
            .map(|item| item.description)
            .collect::<Vec<_>>()
    } else {
        subjects.clone()
    };
    let lowercase = descriptions
        .iter()
        .filter(|text| text.chars().next().is_some_and(char::is_lowercase))
        .count();
    let uppercase = descriptions
        .iter()
        .filter(|text| text.chars().next().is_some_and(char::is_uppercase))
        .count();
    if lowercase * 10 >= descriptions.len() * 8 {
        lines.push("- Start the subject description with a lowercase letter.".to_owned());
    } else if uppercase * 10 >= descriptions.len() * 8 {
        lines.push("- Start the subject description with a capital letter.".to_owned());
    }

    let full_stops = subjects.iter().filter(|s| s.ends_with('.')).count();
    if full_stops * 10 <= subjects.len() {
        lines.push("- Do not end the subject with a full stop.".to_owned());
    }

    let mut lengths = subjects
        .iter()
        .map(|subject| subject.chars().count())
        .collect::<Vec<_>>();
    lengths.sort_unstable();
    let median = lengths[lengths.len() / 2];
    lines.push(format!(
        "- Subjects here are usually about {median} characters long."
    ));

    Some(format!(
        "Match this repository's existing style (from its last {} commits):\n{}",
        subjects.len(),
        lines.join("\n")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subjects(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_owned()).collect()
    }

    #[test]
    fn parses_gitmoji_and_conventional_subjects() {
        let parsed = parse_subject("✨ feat(ai): stream completions").unwrap();
        assert_eq!(parsed.scope, Some("ai"));
        assert_eq!(parsed.description, "stream completions");
        let parsed = parse_subject(":bug: fix!: handle errors").unwrap();
        assert_eq!(parsed.scope, None);
        assert_eq!(parsed.description, "handle errors");
        assert!(parse_subject("Update README").is_none());
    }

    #[test]
    fn infers_scopes_case_and_length() {
        let style = infer_repo_style(&subjects(&[
            "✨ feat(ai): add streaming",
            "🐛 fix(ai): keep provider output",
            "📝 docs(release): publish notes",
            "✨ feat(tui): add split screen",
            "🐛 fix(tui): clamp selection",
            "♻️ refactor(ai): rename complete",
            "✅ test(cli): skip on windows",
            "👷 ci(winget): sync fork",
            "Merge branch 'main'",
        ]))
        .unwrap();
        assert!(style.contains("last 8 commits"));
        assert!(style.contains("Scopes already used here: ai, tui."));
        assert!(style.contains("lowercase"));
        assert!(style.contains("full stop"));
        assert!(style.contains("about"));
    }

    #[test]
    fn needs_enough_history() {
        assert!(infer_repo_style(&subjects(&["feat: one", "fix: two"])).is_none());
    }

    #[test]
    fn handles_free_form_history() {
        let style = infer_repo_style(&subjects(&[
            "Add login page.",
            "Fix crash on start.",
            "Update docs.",
            "Refactor parser.",
            "Bump version.",
            "Add tests.",
            "Remove dead code.",
            "Tidy CI.",
        ]))
        .unwrap();
        assert!(style.contains("capital letter"));
        assert!(!style.contains("full stop"));
        assert!(!style.contains("Scopes"));
    }
}

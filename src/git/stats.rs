use std::collections::BTreeMap;

use anyhow::Result;

use super::{
    exec::run_git_in,
    repo::{parse_lines, repo_root},
};

/// Per-file addition and deletion counts from `git log --numstat`.
#[derive(Debug, Clone, Default)]
pub struct FileStats {
    pub additions: usize,
    pub deletions: usize,
}

/// A single commit with its ISO-8601 timestamp, subject, body, and changed files.
#[derive(Debug, Clone)]
pub struct TimestampedCommit {
    pub hash: String,
    pub subject: String,
    pub body: String,
    pub timestamp: String,
    pub files: Vec<String>,
}

/// Return per-file add/delete stats aggregated over the last `n` commits.
pub fn numstat_last_n(n: usize) -> Result<BTreeMap<String, FileStats>> {
    let root = repo_root()?;
    let output = run_git_in(&root, ["log", &format!("-{n}"), "--numstat", "--format="])?;
    Ok(parse_numstat(&output.stdout))
}

/// Return per-file modification frequency over the last `n` commits.
pub fn file_change_frequency(n: usize) -> Result<BTreeMap<String, usize>> {
    let root = repo_root()?;
    let output = run_git_in(&root, ["log", &format!("-{n}"), "--name-only", "--format="])?;
    let mut freq: BTreeMap<String, usize> = BTreeMap::new();
    for file in parse_lines(&output.stdout) {
        *freq.entry(file).or_insert(0) += 1;
    }
    Ok(freq)
}

/// Return commits with timestamps for the last `n` commits.
pub fn timestamped_commits(n: usize) -> Result<Vec<TimestampedCommit>> {
    let root = repo_root()?;
    let output = run_git_in(
        &root,
        [
            "log",
            &format!("-{n}"),
            "--format=%H%x00%s%x00%aI%x00%b%x00--AIC-TS--",
            "--name-only",
        ],
    )?;
    Ok(parse_timestamped_commits(&output.stdout))
}

/// List all tracked files in the repository.
pub fn tracked_files() -> Result<Vec<String>> {
    let root = repo_root()?;
    let output = run_git_in(&root, ["ls-files"])?;
    Ok(parse_lines(&output.stdout))
}

/// Count lines in a tracked file. Returns 0 for binary/missing files.
pub fn count_file_lines(relative_path: &str) -> Result<usize> {
    let root = repo_root()?;
    let full = root.join(relative_path);
    match std::fs::read_to_string(&full) {
        Ok(contents) => Ok(contents.lines().count()),
        Err(_) => Ok(0),
    }
}

fn parse_numstat(input: &str) -> BTreeMap<String, FileStats> {
    let mut stats: BTreeMap<String, FileStats> = BTreeMap::new();
    for line in input.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() != 3 {
            continue;
        }
        let additions = parts[0].parse::<usize>().unwrap_or(0);
        let deletions = parts[1].parse::<usize>().unwrap_or(0);
        let file = parts[2].to_owned();
        let entry = stats.entry(file).or_default();
        entry.additions += additions;
        entry.deletions += deletions;
    }
    stats
}

fn parse_timestamped_commits(input: &str) -> Vec<TimestampedCommit> {
    // Format: hash\0subject\0timestamp\0body_maybe_multiline\0--AIC-TS--\nfile1\nfile2\n...
    // Split by --AIC-TS-- delimiter. Each block except the last contains one commit's
    // header fields and the previous commit's file list.
    let blocks: Vec<&str> = input.split("--AIC-TS--").collect();

    let mut raw_commits: Vec<ParsedHeader> = Vec::new();
    let mut file_groups: Vec<Vec<String>> = Vec::new();

    for (i, block) in blocks.iter().enumerate() {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }

        if i == 0 {
            // First block: header only (hash\0subject\0timestamp\0body)
            if let Some(rc) = parse_header_block(block) {
                raw_commits.push(rc);
            }
        } else {
            // Subsequent blocks: file list from prev commit, then optionally a new header.
            // The header starts at the first line containing \0.
            let lines: Vec<&str> = block.lines().collect();
            let header_idx = lines.iter().position(|l| l.contains('\0'));
            let (file_lines, header_lines) = match header_idx {
                Some(idx) => (&lines[..idx], Some(&lines[idx..])),
                None => (lines.as_slice(), None),
            };

            let files: Vec<String> = file_lines
                .iter()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty())
                .map(|l| l.to_owned())
                .collect();
            file_groups.push(files);

            if let Some(hdr_lines) = header_lines {
                let rejoined = hdr_lines.join("\n");
                if let Some(rc) = parse_header_block(&rejoined) {
                    raw_commits.push(rc);
                }
            }
        }
    }

    // Last commit may not have a file group yet
    while file_groups.len() < raw_commits.len() {
        file_groups.push(Vec::new());
    }

    let mut commits: Vec<TimestampedCommit> = raw_commits
        .into_iter()
        .zip(file_groups)
        .map(|(rc, files)| TimestampedCommit {
            hash: rc.hash,
            subject: rc.subject,
            body: rc.body,
            timestamp: rc.timestamp,
            files,
        })
        .collect();

    commits.reverse();
    commits
}

/// Parse a block containing: hash\0subject\0timestamp\0body(multiline)
/// The body may contain newlines; it's everything after the third \0.
fn parse_header_block(block: &str) -> Option<ParsedHeader> {
    let mut parts = block.splitn(4, '\0');
    let hash = parts.next()?.trim();
    let subject = parts.next()?.trim();
    let timestamp = parts.next()?.trim();
    let body = parts.next().unwrap_or("").trim().trim_matches('\0').trim();

    if hash.is_empty() || subject.is_empty() {
        return None;
    }

    Some(ParsedHeader {
        hash: hash.to_owned(),
        subject: subject.to_owned(),
        body: body.to_owned(),
        timestamp: timestamp.to_owned(),
    })
}

struct ParsedHeader {
    hash: String,
    subject: String,
    body: String,
    timestamp: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_numstat_extracts_file_stats() {
        let input = "10\t2\tsrc/main.rs\n5\t0\tsrc/lib.rs\n3\t1\tsrc/main.rs\n";
        let stats = parse_numstat(input);
        assert_eq!(stats.len(), 2);
        assert_eq!(stats["src/main.rs"].additions, 13);
        assert_eq!(stats["src/main.rs"].deletions, 3);
        assert_eq!(stats["src/lib.rs"].additions, 5);
    }

    #[test]
    fn parse_numstat_handles_binary_dashes() {
        let input = "-\t-\timage.png\n5\t1\tsrc/lib.rs\n";
        let stats = parse_numstat(input);
        assert_eq!(stats["image.png"].additions, 0);
        assert_eq!(stats["src/lib.rs"].additions, 5);
    }

    #[test]
    fn parse_timestamped_commits_parses_blocks() {
        // git log outputs newest-first; the parser reverses to oldest-first
        // format: hash\0subject\0timestamp\0body\0--AIC-TS--
        let input = "\
def456\x00fix: bug\x002026-04-11T12:00:00+00:00\x00- fixed the bug\x00--AIC-TS--
src/lib.rs
src/cli.rs
abc123\x00feat: init\x002026-04-10T10:00:00+00:00\x00\x00--AIC-TS--
src/main.rs
";
        let commits = parse_timestamped_commits(input);
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].hash, "abc123");
        assert_eq!(commits[0].body, "");
        assert_eq!(commits[0].timestamp, "2026-04-10T10:00:00+00:00");
        assert_eq!(commits[0].files, vec!["src/main.rs"]);
        assert_eq!(commits[1].hash, "def456");
        assert_eq!(commits[1].body, "- fixed the bug");
        assert_eq!(commits[1].files.len(), 2);
    }

    #[test]
    fn parse_timestamped_commits_handles_multiline_body() {
        let input = "\
abc123\x00feat: init\x002026-04-10T10:00:00+00:00\x00- line one\n- line two\x00--AIC-TS--
src/main.rs
";
        let commits = parse_timestamped_commits(input);
        assert_eq!(commits.len(), 1);
        assert!(commits[0].body.contains("line one"));
        assert!(commits[0].body.contains("line two"));
    }
}

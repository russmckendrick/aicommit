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

/// A single commit with its ISO-8601 timestamp and subject.
#[derive(Debug, Clone)]
pub struct TimestampedCommit {
    pub hash: String,
    pub subject: String,
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
            "--format=%H%x00%s%x00%aI%x00--AIC-TS--",
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
    // git output: header\0--AIC-TS--\nfile1\nfile2\n\nheader\0--AIC-TS--\n...
    // Split by the delimiter; block 0 has the first header, block 1+ has
    // [files from prev commit]\n[next header] etc.
    let blocks: Vec<&str> = input.split("--AIC-TS--").collect();
    let mut headers: Vec<(&str, &str, &str)> = Vec::new();
    let mut file_groups: Vec<Vec<String>> = Vec::new();

    for (i, block) in blocks.iter().enumerate() {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }

        if i == 0 {
            // First block: just a header line
            if let Some((h, s, t)) = parse_header(block) {
                headers.push((h, s, t));
            }
        } else {
            // Subsequent blocks: files from previous commit, then optionally a new header
            let lines: Vec<&str> = block.lines().collect();
            // Find the header line (contains \0)
            let header_idx = lines.iter().position(|l| l.contains('\0'));
            let (file_lines, header_line) = match header_idx {
                Some(idx) => (&lines[..idx], Some(lines[idx])),
                None => (lines.as_slice(), None),
            };

            let files: Vec<String> = file_lines
                .iter()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty())
                .map(|l| l.to_owned())
                .collect();
            file_groups.push(files);

            if let Some(line) = header_line
                && let Some((h, s, t)) = parse_header(line)
            {
                headers.push((h, s, t));
            }
        }
    }

    // Last header may not have a corresponding file group yet
    while file_groups.len() < headers.len() {
        file_groups.push(Vec::new());
    }

    let mut commits: Vec<TimestampedCommit> = headers
        .into_iter()
        .zip(file_groups)
        .map(|((hash, subject, timestamp), files)| TimestampedCommit {
            hash: hash.to_owned(),
            subject: subject.to_owned(),
            timestamp: timestamp.to_owned(),
            files,
        })
        .collect();

    commits.reverse();
    commits
}

fn parse_header(line: &str) -> Option<(&str, &str, &str)> {
    let parts: Vec<&str> = line.split('\0').collect();
    if parts.len() >= 3 {
        Some((parts[0].trim(), parts[1].trim(), parts[2].trim()))
    } else {
        None
    }
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
        let input = "\
def456\x00fix: bug\x002026-04-11T12:00:00+00:00\x00--AIC-TS--
src/lib.rs
src/cli.rs
abc123\x00feat: init\x002026-04-10T10:00:00+00:00\x00--AIC-TS--
src/main.rs
";
        let commits = parse_timestamped_commits(input);
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].hash, "abc123");
        assert_eq!(commits[0].timestamp, "2026-04-10T10:00:00+00:00");
        assert_eq!(commits[0].files, vec!["src/main.rs"]);
        assert_eq!(commits[1].hash, "def456");
        assert_eq!(commits[1].files.len(), 2);
    }
}

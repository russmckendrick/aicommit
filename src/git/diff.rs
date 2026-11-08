//! Hunk-level model of the staged diff, and the index operations that let a
//! split commit stage any subset of hunks without touching the working tree.
//!
//! Each hunk gets a stable ID of the form `path#n` (1-based, in diff order).
//! Changes that cannot be split meaningfully (new, deleted, renamed, copied,
//! mode-changed, or binary files) are "atomic": the whole file section counts
//! as one hunk, `path#1`.

use std::{
    collections::BTreeSet,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

use anyhow::{Context, Result, bail};

use super::repo::repo_root;

/// One `@@` hunk, including its header line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDiff {
    /// Repository-relative path (the new path for renames and copies).
    pub path: String,
    /// `diff --git` line through the `+++` line (or the whole extended
    /// header when the section has no hunks).
    pub header: String,
    pub hunks: Vec<Hunk>,
    /// When true the whole section is one indivisible change.
    pub atomic: bool,
    /// Full section text, used verbatim when the file is atomic.
    pub raw: String,
}

impl FileDiff {
    pub fn hunk_ids(&self) -> Vec<String> {
        if self.atomic || self.hunks.is_empty() {
            vec![hunk_id(&self.path, 1)]
        } else {
            self.hunks.iter().map(|hunk| hunk.id.clone()).collect()
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StagedDiff {
    pub files: Vec<FileDiff>,
}

pub fn hunk_id(path: &str, index: usize) -> String {
    format!("{path}#{index}")
}

/// Split `path#n` back into its path; IDs without a `#n` suffix are paths.
pub fn hunk_path(id: &str) -> &str {
    match id.rsplit_once('#') {
        Some((path, n)) if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) => path,
        _ => id,
    }
}

impl StagedDiff {
    pub fn file(&self, path: &str) -> Option<&FileDiff> {
        self.files.iter().find(|file| file.path == path)
    }

    /// Every hunk ID, in diff order.
    pub fn hunk_ids(&self) -> Vec<String> {
        self.files.iter().flat_map(FileDiff::hunk_ids).collect()
    }

    /// Hunk IDs belonging to `paths`, in diff order.
    pub fn hunk_ids_for_files(&self, paths: &[String]) -> Vec<String> {
        self.files
            .iter()
            .filter(|file| paths.contains(&file.path))
            .flat_map(FileDiff::hunk_ids)
            .collect()
    }

    /// Distinct file paths touched by `ids`, in diff order.
    pub fn files_for(&self, ids: &[String]) -> Vec<String> {
        let paths = ids.iter().map(|id| hunk_path(id)).collect::<BTreeSet<_>>();
        self.files
            .iter()
            .filter(|file| paths.contains(file.path.as_str()))
            .map(|file| file.path.clone())
            .collect()
    }

    /// A patch containing exactly the selected hunks, for `git apply --cached`.
    pub fn patch_for(&self, ids: &[String]) -> String {
        let selected = ids.iter().map(String::as_str).collect::<BTreeSet<_>>();
        let mut patch = String::new();
        for file in &self.files {
            if file.atomic || file.hunks.is_empty() {
                if selected.contains(hunk_id(&file.path, 1).as_str()) {
                    push_block(&mut patch, &file.raw);
                }
                continue;
            }
            let hunks = file
                .hunks
                .iter()
                .filter(|hunk| selected.contains(hunk.id.as_str()))
                .collect::<Vec<_>>();
            if hunks.is_empty() {
                continue;
            }
            push_block(&mut patch, &file.header);
            for hunk in hunks {
                push_block(&mut patch, &hunk.text);
            }
        }
        patch
    }

    /// Diff text for the model: each hunk is preceded by a `### hunk <id>`
    /// marker line so the model can assign hunks by ID. Files for which
    /// `omit_content` returns true (lock files, images, binaries) show only
    /// their marker.
    pub fn annotated(&self, omit_content: impl Fn(&FileDiff) -> bool) -> String {
        let mut out = String::new();
        for file in &self.files {
            if omit_content(file) || file.atomic && is_binary_section(&file.raw) {
                push_block(
                    &mut out,
                    &format!(
                        "### hunk {} (content omitted: {})",
                        hunk_id(&file.path, 1),
                        file.path
                    ),
                );
                continue;
            }
            if file.atomic || file.hunks.is_empty() {
                push_block(&mut out, &format!("### hunk {}", hunk_id(&file.path, 1)));
                push_block(&mut out, &file.raw);
                continue;
            }
            push_block(&mut out, &file.header);
            for hunk in &file.hunks {
                push_block(&mut out, &format!("### hunk {}", hunk.id));
                push_block(&mut out, &hunk.text);
            }
        }
        out
    }

    /// Plain diff text for the selected hunks, skipping omitted files. Used
    /// to draft one commit message per split group.
    pub fn display_for(&self, ids: &[String], omit_content: impl Fn(&FileDiff) -> bool) -> String {
        let visible = StagedDiff {
            files: self
                .files
                .iter()
                .filter(|file| !omit_content(file) && !is_binary_section(&file.raw))
                .cloned()
                .collect(),
        };
        visible.patch_for(ids)
    }

    /// Paths split across more than one of `groups`, with the hunk numbers
    /// each group holds, for display.
    pub fn shared_files(&self, groups: &[Vec<String>]) -> Vec<String> {
        let mut shared = Vec::new();
        for file in &self.files {
            let owners = groups
                .iter()
                .filter(|ids| ids.iter().any(|id| hunk_path(id) == file.path))
                .count();
            if owners > 1 {
                shared.push(file.path.clone());
            }
        }
        shared
    }
}

fn push_block(out: &mut String, block: &str) {
    out.push_str(block);
    if !block.ends_with('\n') {
        out.push('\n');
    }
}

fn is_binary_section(raw: &str) -> bool {
    raw.lines()
        .any(|line| line.starts_with("Binary files ") || line == "GIT binary patch")
}

/// Parse unified `git diff` output into files and hunks.
pub fn parse_diff(input: &str) -> StagedDiff {
    let mut files = Vec::new();
    let mut sections: Vec<Vec<&str>> = Vec::new();
    for line in input.split_inclusive('\n') {
        if line.starts_with("diff --git ") || sections.is_empty() {
            sections.push(Vec::new());
        }
        if let Some(section) = sections.last_mut() {
            section.push(line);
        }
    }

    for section in sections {
        if section
            .first()
            .is_none_or(|line| !line.starts_with("diff --git "))
        {
            continue;
        }
        files.push(parse_section(&section));
    }

    StagedDiff { files }
}

fn parse_section(lines: &[&str]) -> FileDiff {
    let raw = lines.concat();
    let first_hunk = lines.iter().position(|line| line.starts_with("@@"));
    let header_lines = &lines[..first_hunk.unwrap_or(lines.len())];
    let header = header_lines.concat();

    let mut path = None;
    let mut old_path = None;
    let mut structural = false;
    for line in header_lines {
        let line = line.trim_end_matches(['\n', '\r']);
        if let Some(rest) = line.strip_prefix("+++ ") {
            if rest != "/dev/null" {
                path = Some(strip_prefix_dir(
                    &unquote(rest.trim_end_matches('\t')),
                    "b/",
                ));
            }
        } else if let Some(rest) = line.strip_prefix("--- ") {
            if rest != "/dev/null" {
                old_path = Some(strip_prefix_dir(
                    &unquote(rest.trim_end_matches('\t')),
                    "a/",
                ));
            }
        } else if let Some(rest) = line
            .strip_prefix("rename to ")
            .or_else(|| line.strip_prefix("copy to "))
        {
            path = Some(unquote(rest));
            structural = true;
        } else if line.starts_with("new file mode")
            || line.starts_with("deleted file mode")
            || line.starts_with("rename from")
            || line.starts_with("copy from")
            || line.starts_with("old mode")
            || line.starts_with("Binary files ")
            || line == "GIT binary patch"
        {
            structural = true;
        }
    }

    let path = path
        .or(old_path)
        .or_else(|| path_from_diff_git_line(lines[0]))
        .unwrap_or_default();

    let mut hunks = Vec::new();
    if let Some(start) = first_hunk {
        let mut current: Option<String> = None;
        for line in &lines[start..] {
            if line.starts_with("@@") {
                if let Some(text) = current.take() {
                    hunks.push(Hunk {
                        id: hunk_id(&path, hunks.len() + 1),
                        text,
                    });
                }
                current = Some((*line).to_owned());
            } else if let Some(text) = current.as_mut() {
                text.push_str(line);
            }
        }
        if let Some(text) = current {
            hunks.push(Hunk {
                id: hunk_id(&path, hunks.len() + 1),
                text,
            });
        }
    }

    let atomic = structural || hunks.is_empty();
    FileDiff {
        path,
        header,
        hunks,
        atomic,
        raw,
    }
}

fn strip_prefix_dir(path: &str, prefix: &str) -> String {
    path.strip_prefix(prefix).unwrap_or(path).to_owned()
}

/// `diff --git a/P b/P` with identical, unquoted paths (the case where no
/// `---`/`+++` lines exist, such as a mode-only change).
fn path_from_diff_git_line(line: &str) -> Option<String> {
    let rest = line
        .trim_end_matches(['\n', '\r'])
        .strip_prefix("diff --git ")?;
    if rest.starts_with('"') {
        let (first, _) = split_quoted(rest)?;
        return Some(strip_prefix_dir(&first, "a/"));
    }
    if rest.len() < 5 || (rest.len() - 5) % 2 != 0 {
        return None;
    }
    let len = (rest.len() - 5) / 2;
    let old = rest.get(2..2 + len)?;
    let new = rest.get(2 + len + 3..)?;
    (rest.starts_with("a/") && old == new).then(|| old.to_owned())
}

fn split_quoted(input: &str) -> Option<(String, &str)> {
    let mut escaped = false;
    for (index, ch) in input.char_indices().skip(1) {
        match ch {
            '\\' if !escaped => escaped = true,
            '"' if !escaped => {
                return Some((unquote(&input[..=index]), &input[index + 1..]));
            }
            _ => escaped = false,
        }
    }
    None
}

/// Undo Git's C-style path quoting (`"a/caf\303\251.txt"`).
fn unquote(input: &str) -> String {
    let Some(inner) = input
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
    else {
        return input.to_owned();
    };

    let mut bytes = Vec::with_capacity(inner.len());
    let mut chars = inner.bytes().peekable();
    while let Some(byte) = chars.next() {
        if byte != b'\\' {
            bytes.push(byte);
            continue;
        }
        match chars.next() {
            Some(b'n') => bytes.push(b'\n'),
            Some(b't') => bytes.push(b'\t'),
            Some(b'"') => bytes.push(b'"'),
            Some(b'\\') => bytes.push(b'\\'),
            Some(digit @ b'0'..=b'7') => {
                let mut value = u32::from(digit - b'0');
                for _ in 0..2 {
                    match chars.peek() {
                        Some(next @ b'0'..=b'7') => {
                            value = value * 8 + u32::from(next - b'0');
                            chars.next();
                        }
                        _ => break,
                    }
                }
                bytes.push(value as u8);
            }
            Some(other) => {
                bytes.push(b'\\');
                bytes.push(other);
            }
            None => bytes.push(b'\\'),
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

// ---------------------------------------------------------------------------
// Index operations
// ---------------------------------------------------------------------------

fn git_raw(root: &Path, args: &[&str], stdin: Option<&str>) -> Result<String> {
    let mut command = Command::new("git");
    command
        .args(args)
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if stdin.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command
        .spawn()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;
    if let Some(input) = stdin
        && let Some(mut pipe) = child.stdin.take()
    {
        pipe.write_all(input.as_bytes())?;
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        bail!("git {} failed: {stderr}", args.join(" "));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The staged diff against `HEAD` (or the empty tree), with settings fixed
/// so user `diff.*` config cannot change the format, and binary changes
/// included so the patch can be re-applied exactly.
pub fn staged_patch() -> Result<StagedDiff> {
    let root = repo_root()?;
    let text = git_raw(
        &root,
        &[
            "diff",
            "--cached",
            "--binary",
            "--full-index",
            "--find-renames",
            "--no-color",
            "--no-ext-diff",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            "--unified=3",
        ],
        None,
    )?;
    Ok(parse_diff(&text))
}

/// Record the current index as a tree object and return its ID.
pub fn write_tree() -> Result<String> {
    let root = repo_root()?;
    Ok(git_raw(&root, &["write-tree"], None)?.trim().to_owned())
}

/// Replace the index with `tree`. The working tree is not touched.
pub fn read_tree(tree: &str) -> Result<()> {
    let root = repo_root()?;
    git_raw(&root, &["read-tree", tree], None)?;
    Ok(())
}

/// Reset the index to `HEAD`, or empty it in a repository with no commits.
pub fn reset_index_to_head() -> Result<()> {
    let root = repo_root()?;
    if git_raw(&root, &["rev-parse", "--verify", "--quiet", "HEAD"], None).is_ok() {
        git_raw(&root, &["read-tree", "HEAD"], None)?;
    } else {
        git_raw(&root, &["read-tree", "--empty"], None)?;
    }
    Ok(())
}

/// Apply `patch` to the index only.
pub fn apply_to_index(patch: &str) -> Result<()> {
    if patch.trim().is_empty() {
        return Ok(());
    }
    let root = repo_root()?;
    git_raw(
        &root,
        &["apply", "--cached", "--whitespace=nowarn", "-"],
        Some(patch),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TWO_HUNKS: &str = "diff --git a/src/ui.rs b/src/ui.rs
index 1111111..2222222 100644
--- a/src/ui.rs
+++ b/src/ui.rs
@@ -1,3 +1,3 @@
 one
-two
+TWO
 three
@@ -20,3 +20,4 @@ fn later() {
 twenty
 twenty-one
+twenty-one-and-a-half
 twenty-two
diff --git a/docs/new.md b/docs/new.md
new file mode 100644
index 0000000..3333333
--- /dev/null
+++ b/docs/new.md
@@ -0,0 +1 @@
+hello
diff --git a/old.txt b/renamed.txt
similarity index 100%
rename from old.txt
rename to renamed.txt
diff --git a/image.png b/image.png
new file mode 100644
index 0000000..4444444
Binary files /dev/null and b/image.png differ
diff --git a/script.sh b/script.sh
old mode 100644
new mode 100755
";

    #[test]
    fn parses_files_hunks_and_atomic_sections() {
        let diff = parse_diff(TWO_HUNKS);
        let paths = diff
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            paths,
            vec![
                "src/ui.rs",
                "docs/new.md",
                "renamed.txt",
                "image.png",
                "script.sh"
            ]
        );
        assert_eq!(
            diff.hunk_ids(),
            vec![
                "src/ui.rs#1",
                "src/ui.rs#2",
                "docs/new.md#1",
                "renamed.txt#1",
                "image.png#1",
                "script.sh#1"
            ]
        );
        assert!(!diff.files[0].atomic);
        assert!(diff.files[1..].iter().all(|file| file.atomic));
    }

    #[test]
    fn builds_a_patch_from_selected_hunks() {
        let diff = parse_diff(TWO_HUNKS);
        let patch = diff.patch_for(&["src/ui.rs#2".to_owned(), "docs/new.md#1".to_owned()]);
        assert!(patch.starts_with("diff --git a/src/ui.rs b/src/ui.rs\n"));
        assert!(patch.contains("+twenty-one-and-a-half"));
        assert!(!patch.contains("+TWO"));
        assert!(patch.contains("+++ b/docs/new.md\n@@ -0,0 +1 @@\n+hello\n"));
        assert!(!patch.contains("renamed.txt"));
    }

    #[test]
    fn annotates_hunks_and_omits_binary_content() {
        let diff = parse_diff(TWO_HUNKS);
        let annotated = diff.annotated(|_| false);
        assert!(annotated.contains("### hunk src/ui.rs#1\n@@ -1,3 +1,3 @@"));
        assert!(annotated.contains("### hunk src/ui.rs#2\n@@ -20,3 +20,4 @@"));
        assert!(annotated.contains("### hunk image.png#1 (content omitted: image.png)"));
        assert!(!annotated.contains("Binary files"));
    }

    #[test]
    fn maps_hunks_back_to_files_and_finds_shared_files() {
        let diff = parse_diff(TWO_HUNKS);
        assert_eq!(
            diff.files_for(&["src/ui.rs#2".to_owned(), "image.png#1".to_owned()]),
            vec!["src/ui.rs", "image.png"]
        );
        assert_eq!(
            diff.shared_files(&[
                vec!["src/ui.rs#1".to_owned()],
                vec!["src/ui.rs#2".to_owned(), "docs/new.md#1".to_owned()],
            ]),
            vec!["src/ui.rs"]
        );
        assert_eq!(hunk_path("a#b/c.rs#12"), "a#b/c.rs");
        assert_eq!(hunk_path("plain.rs"), "plain.rs");
    }

    #[test]
    fn unquotes_git_paths() {
        assert_eq!(unquote("\"b/caf\\303\\251 x.txt\""), "b/café x.txt");
        assert_eq!(unquote("b/plain.txt"), "b/plain.txt");
        assert_eq!(
            path_from_diff_git_line("diff --git a/x y.rs b/x y.rs\n").as_deref(),
            Some("x y.rs")
        );
    }
}

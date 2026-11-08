//! TUI state and event handling. Rendering lives in `view.rs`; Git and AI
//! work goes through `crate::git` and `crate::generator`, never directly.

use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::{Position, Rect};
use tokio::{sync::mpsc, task::JoinHandle};

use crate::{
    config::Config,
    generator::{self, GenerationProgress},
    git::{self, StagedDiff, diff::hunk_path},
    prompt::SplitPlanGroup,
};

use super::{
    editor::TextEditor,
    settings::{self, FieldKind, SettingsState},
    theme::Palette,
};

pub const MAX_GROUPS: usize = 6;
const REGENERATE_DEBOUNCE: Duration = Duration::from_millis(700);
const TOAST_DURATION: Duration = Duration::from_millis(2600);

/// What the user asked for when the TUI closed.
#[derive(Debug)]
pub enum Outcome {
    Commit {
        message: String,
        files: Vec<String>,
    },
    Split {
        staged: StagedDiff,
        groups: Vec<(SplitPlanGroup, String)>,
    },
    Quit,
}

pub struct TuiInput {
    pub config: Config,
    /// Open on the settings screen because no API key is configured.
    pub first_run: bool,
    pub extra_args: Vec<String>,
    /// Context already enriched with branch information.
    pub context: String,
    pub full_gitmoji_spec: bool,
}

#[derive(Debug)]
pub enum AppEvent {
    Progress(u64, GenerationProgress),
    Delta(u64, String),
    MessageDone(u64, Result<String, String>),
    PlanDone(u64, Result<Vec<SplitPlanGroup>, String>),
    GroupMessage {
        group_gen: u64,
        result: Result<String, String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowKind {
    Section {
        title: String,
    },
    File {
        path: String,
        staged: bool,
        marker: FileMarker,
    },
    Hunk {
        path: String,
        id: String,
        staged: bool,
        label: String,
        stats: (usize, usize),
    },
    Untracked {
        path: String,
    },
    Empty {
        text: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileMarker {
    Full,
    Partial,
    Unstaged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Tree,
    Message,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Commit,
    Split,
    Settings,
}

pub enum Gen {
    Idle,
    Running {
        id: u64,
        stage: String,
        started: Instant,
        handle: JoinHandle<()>,
    },
    Failed(String),
}

pub struct GroupState {
    pub title: String,
    pub rationale: String,
    pub hunks: Vec<String>,
    pub message: String,
    pub generating: bool,
    pub stale: bool,
    pub gen_id: u64,
}

pub enum SplitStatus {
    Planning { started: Instant },
    Ready,
    Failed(String),
}

pub struct SplitState {
    pub staged: StagedDiff,
    pub groups: Vec<GroupState>,
    pub col: usize,
    pub row: usize,
    pub appeared: Instant,
    pub dirty_at: Option<Instant>,
    pub status: SplitStatus,
    pub plan_id: u64,
    pub editing: Option<TextEditor>,
    pub drag: Option<(usize, usize)>,
}

impl SplitState {
    pub fn selected_hunk(&self) -> Option<&String> {
        self.groups.get(self.col)?.hunks.get(self.row)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Generate,
    Edit,
    ToggleStage,
    StageAll,
    UnstageAll,
    Split,
    Commit,
    Theme,
    Settings,
    Help,
    Quit,
}

impl Action {
    pub const ALL: [Action; 11] = [
        Action::Generate,
        Action::Edit,
        Action::ToggleStage,
        Action::StageAll,
        Action::UnstageAll,
        Action::Split,
        Action::Commit,
        Action::Theme,
        Action::Settings,
        Action::Help,
        Action::Quit,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Action::Generate => "Generate commit message",
            Action::Edit => "Edit message",
            Action::ToggleStage => "Stage or unstage selection",
            Action::StageAll => "Stage all changes",
            Action::UnstageAll => "Unstage everything",
            Action::Split => "Split into several commits",
            Action::Commit => "Commit",
            Action::Theme => "Next theme",
            Action::Settings => "Settings: provider, model, API key, style",
            Action::Help => "Keyboard shortcuts",
            Action::Quit => "Quit without committing",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Action::Generate => "g",
            Action::Edit => "e",
            Action::ToggleStage => "space",
            Action::StageAll => "a",
            Action::UnstageAll => "u",
            Action::Split => "s",
            Action::Commit => "c",
            Action::Theme => "t",
            Action::Settings => ",",
            Action::Help => "?",
            Action::Quit => "q",
        }
    }
}

pub struct PaletteState {
    pub query: String,
    pub selected: usize,
}

impl PaletteState {
    pub fn matches(&self) -> Vec<Action> {
        let query = self.query.to_lowercase();
        Action::ALL
            .into_iter()
            .filter(|action| fuzzy_match(&action.label().to_lowercase(), &query))
            .collect()
    }
}

fn fuzzy_match(haystack: &str, needle: &str) -> bool {
    let mut chars = haystack.chars();
    needle.chars().all(|n| chars.any(|h| h == n))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    TreeRow(usize),
    Diff,
    Message,
    SplitCard(usize, usize),
    SplitColumn(usize),
}

pub struct App {
    pub input: TuiInput,
    pub palette: Palette,
    pub theme_names: Vec<String>,
    pub screen: Screen,
    pub focus: Focus,
    pub staged: StagedDiff,
    pub unstaged: StagedDiff,
    pub untracked: Vec<String>,
    pub staged_files: Vec<String>,
    pub rows: Vec<RowKind>,
    pub selected: usize,
    pub diff_scroll: u16,
    pub diff_height: u16,
    pub message: TextEditor,
    pub streamed: String,
    pub generation: Gen,
    pub message_stale: bool,
    pub prompt_tokens: usize,
    pub branch: Option<String>,
    pub ticket: Option<String>,
    pub split: Option<SplitState>,
    pub palette_overlay: Option<PaletteState>,
    pub help: bool,
    pub toast: Option<(String, bool, Instant)>,
    pub hits: Vec<(Rect, Hit)>,
    pub started: Instant,
    pub outcome: Option<Outcome>,
    pub settings: Option<SettingsState>,
    pub config_path: std::path::PathBuf,
    /// Force a full repaint on the next frame (after streaming, theme
    /// changes, or screen switches) so terminals that disagree about emoji
    /// widths never keep stale cells.
    pub needs_clear: bool,
    next_id: u64,
    pub tx: mpsc::UnboundedSender<AppEvent>,
}

impl App {
    pub fn new(input: TuiInput, tx: mpsc::UnboundedSender<AppEvent>) -> anyhow::Result<Self> {
        let palette = super::theme::palette(&input.config.theme);
        let mut app = Self {
            palette,
            theme_names: crate::map::theme::available_theme_names(),
            screen: Screen::Commit,
            focus: Focus::Tree,
            staged: StagedDiff::default(),
            unstaged: StagedDiff::default(),
            untracked: Vec::new(),
            staged_files: Vec::new(),
            rows: Vec::new(),
            selected: 0,
            diff_scroll: 0,
            diff_height: 20,
            message: TextEditor::default(),
            streamed: String::new(),
            generation: Gen::Idle,
            message_stale: false,
            prompt_tokens: 0,
            branch: git::current_branch(),
            ticket: git::ticket_from_branch(),
            split: None,
            palette_overlay: None,
            help: false,
            toast: None,
            hits: Vec::new(),
            started: Instant::now(),
            outcome: None,
            settings: None,
            config_path: crate::config::ConfigPaths::discover()
                .map(|paths| paths.global)
                .unwrap_or_else(|_| std::path::PathBuf::from(".aicommit")),
            needs_clear: false,
            next_id: 0,
            tx,
            input,
        };
        app.reload()?;
        if app.input.first_run {
            app.open_settings(true);
        }
        Ok(app)
    }

    fn next_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    pub fn toast(&mut self, message: impl Into<String>, error: bool) {
        self.toast = Some((message.into(), error, Instant::now()));
    }

    pub fn is_generating(&self) -> bool {
        matches!(self.generation, Gen::Running { .. })
    }

    /// Whether background work (generation, planning, group drafts) is running.
    pub fn animating_work(&self) -> bool {
        self.is_generating()
            || self.split.as_ref().is_some_and(|split| {
                matches!(split.status, SplitStatus::Planning { .. })
                    || split.groups.iter().any(|group| group.generating)
            })
    }

    /// Whether anything on screen is animating (so the loop keeps ticking).
    pub fn animating(&self) -> bool {
        self.animating_work()
            || self.toast.is_some()
            || self.split.as_ref().is_some_and(|split| {
                split.appeared.elapsed() < Duration::from_millis(900) || split.dirty_at.is_some()
            })
    }

    // -----------------------------------------------------------------
    // Repository state
    // -----------------------------------------------------------------

    pub fn reload(&mut self) -> anyhow::Result<()> {
        let previous = self.rows.get(self.selected).cloned();
        let previous_hashes = self.staged.hunk_ids();
        self.staged = git::staged_patch()?;
        self.unstaged = git::unstaged_patch()?;
        self.untracked = git::untracked_files()?;
        self.staged_files = git::staged_files()?;
        self.rebuild_rows();
        self.restore_selection(previous);
        if previous_hashes != self.staged.hunk_ids() && !self.message.is_empty() {
            self.message_stale = true;
        }
        self.prompt_tokens = if self.staged_files.is_empty() {
            0
        } else {
            crate::commands::commit::staged_commit_input(&self.staged_files)
                .map(|input| crate::token::count_tokens(&input.content))
                .unwrap_or(0)
        };
        Ok(())
    }

    fn rebuild_rows(&mut self) {
        let mut rows = Vec::new();
        let staged_hunks: usize = self.staged.files.iter().map(|f| f.hunk_ids().len()).sum();
        rows.push(RowKind::Section {
            title: format!(
                "Staged · {} · {}",
                plural(self.staged.files.len(), "file"),
                plural(staged_hunks, "hunk")
            ),
        });
        if self.staged.files.is_empty() {
            rows.push(RowKind::Empty {
                text: "Nothing staged yet".to_owned(),
            });
        }
        for file in &self.staged.files {
            let partial = self.unstaged.file(&file.path).is_some();
            rows.push(RowKind::File {
                path: file.path.clone(),
                staged: true,
                marker: if partial {
                    FileMarker::Partial
                } else {
                    FileMarker::Full
                },
            });
            push_hunk_rows(&mut rows, file, true);
        }

        let unstaged_count = self.unstaged.files.len() + self.untracked.len();
        rows.push(RowKind::Section {
            title: format!("Unstaged · {}", plural(unstaged_count, "file")),
        });
        if unstaged_count == 0 {
            rows.push(RowKind::Empty {
                text: "Working tree clean".to_owned(),
            });
        }
        for file in &self.unstaged.files {
            rows.push(RowKind::File {
                path: file.path.clone(),
                staged: false,
                marker: if self.staged.file(&file.path).is_some() {
                    FileMarker::Partial
                } else {
                    FileMarker::Unstaged
                },
            });
            push_hunk_rows(&mut rows, file, false);
        }
        for path in &self.untracked {
            rows.push(RowKind::Untracked { path: path.clone() });
        }
        self.rows = rows;
    }

    fn restore_selection(&mut self, previous: Option<RowKind>) {
        let selectable =
            |row: &RowKind| !matches!(row, RowKind::Section { .. } | RowKind::Empty { .. });
        if let Some(previous) = previous {
            if let Some(index) = self.rows.iter().position(|row| *row == previous) {
                self.selected = index;
                return;
            }
            // Same file, any row.
            let path = row_path(&previous);
            if let Some(index) = self
                .rows
                .iter()
                .position(|row| selectable(row) && row_path(row) == path && path.is_some())
            {
                self.selected = index;
                return;
            }
        }
        self.selected = self.selected.min(self.rows.len().saturating_sub(1));
        if !self.rows.get(self.selected).is_some_and(selectable) {
            self.selected = self.rows.iter().position(selectable).unwrap_or(0);
        }
        self.diff_scroll = 0;
    }

    pub fn move_selection(&mut self, delta: isize) {
        let selectable: Vec<usize> = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| !matches!(row, RowKind::Section { .. } | RowKind::Empty { .. }))
            .map(|(index, _)| index)
            .collect();
        if selectable.is_empty() {
            return;
        }
        let current = selectable
            .iter()
            .position(|index| *index == self.selected)
            .unwrap_or(0) as isize;
        let next = (current + delta).clamp(0, selectable.len() as isize - 1) as usize;
        self.selected = selectable[next];
        self.diff_scroll = 0;
    }

    fn toggle_stage(&mut self) {
        let Some(row) = self.rows.get(self.selected).cloned() else {
            return;
        };
        let result = match &row {
            RowKind::Hunk {
                id, staged: true, ..
            } => git::unapply_from_index(&self.staged.patch_for(std::slice::from_ref(id))),
            RowKind::Hunk {
                id, staged: false, ..
            } => git::apply_to_index(&self.unstaged.patch_for(std::slice::from_ref(id))),
            RowKind::File {
                path, staged: true, ..
            } => {
                let ids = self.staged.hunk_ids_for_files(std::slice::from_ref(path));
                git::unapply_from_index(&self.staged.patch_for(&ids))
            }
            RowKind::File {
                path,
                staged: false,
                ..
            } => {
                let ids = self.unstaged.hunk_ids_for_files(std::slice::from_ref(path));
                git::apply_to_index(&self.unstaged.patch_for(&ids))
            }
            RowKind::Untracked { path } => git::add_files(std::slice::from_ref(path)),
            _ => return,
        };
        match result.and_then(|_| self.reload()) {
            Ok(()) => {}
            Err(error) => self.toast(format!("Couldn't change staging: {error}"), true),
        }
    }

    fn stage_all(&mut self, stage: bool) {
        let result = if stage {
            let mut paths = self
                .unstaged
                .files
                .iter()
                .map(|f| f.path.clone())
                .collect::<Vec<_>>();
            paths.extend(self.untracked.iter().cloned());
            git::add_files(&paths)
        } else {
            git::reset_index_to_head()
        };
        if let Err(error) = result.and_then(|_| self.reload()) {
            self.toast(format!("Couldn't change staging: {error}"), true);
        }
    }

    // -----------------------------------------------------------------
    // Generation
    // -----------------------------------------------------------------

    pub fn start_generation(&mut self) {
        if self.staged_files.is_empty() {
            self.toast("Stage some changes first (space on a hunk or file)", true);
            return;
        }
        let input = match crate::commands::commit::staged_commit_input(&self.staged_files) {
            Ok(input) if !input.content.trim().is_empty() => input,
            Ok(_) => {
                self.toast("Nothing readable to describe after ignore filters", true);
                return;
            }
            Err(error) => {
                self.toast(error.to_string(), true);
                return;
            }
        };
        self.cancel_generation();
        let id = self.next_id();
        let tx = self.tx.clone();
        let config = self.input.config.clone();
        let context = self.input.context.clone();
        let gitmoji = self.input.full_gitmoji_spec;
        let extra_args = self.input.extra_args.clone();
        let files = self.staged_files.clone();
        let handle = tokio::spawn(async move {
            let progress_tx = tx.clone();
            let progress = move |event: GenerationProgress| {
                let _ = progress_tx.send(AppEvent::Progress(id, event));
            };
            let delta_tx = tx.clone();
            let on_delta = move |delta: &str| {
                let _ = delta_tx.send(AppEvent::Delta(id, delta.to_owned()));
            };
            let result = generator::generate_commit_message_streaming(
                &config,
                &input.content,
                gitmoji,
                &context,
                &files,
                Some(&progress),
                Some(&on_delta),
            )
            .await
            .map(|message| {
                crate::commands::commit::apply_message_template(&config, &extra_args, &message)
            })
            .map_err(|error| error.to_string());
            let _ = tx.send(AppEvent::MessageDone(id, result));
        });
        self.streamed.clear();
        self.message.set_text("");
        self.message_stale = false;
        self.focus = Focus::Tree;
        self.generation = Gen::Running {
            id,
            stage: "Generating commit message".to_owned(),
            started: Instant::now(),
            handle,
        };
    }

    pub fn cancel_generation(&mut self) {
        if let Gen::Running { handle, .. } = &self.generation {
            handle.abort();
            self.generation = Gen::Idle;
        }
    }

    fn start_split(&mut self) {
        let plannable = StagedDiff {
            files: self
                .staged
                .files
                .iter()
                .filter(|file| self.staged_files.contains(&file.path))
                .cloned()
                .collect(),
        };
        if plannable.hunk_ids().len() < 2 {
            self.toast("Splitting needs at least two staged hunks", true);
            return;
        }
        let id = self.next_id();
        let tx = self.tx.clone();
        let config = self.input.config.clone();
        let context = self.input.context.clone();
        let plan_input = plannable.clone();
        tokio::spawn(async move {
            let result = generator::generate_split_plan(&config, &plan_input, &context, None)
                .await
                .map_err(|error| error.to_string());
            let _ = tx.send(AppEvent::PlanDone(id, result));
        });
        self.split = Some(SplitState {
            staged: plannable,
            groups: Vec::new(),
            col: 0,
            row: 0,
            appeared: Instant::now(),
            dirty_at: None,
            status: SplitStatus::Planning {
                started: Instant::now(),
            },
            plan_id: id,
            editing: None,
            drag: None,
        });
        self.screen = Screen::Split;
    }

    fn generate_group_messages(&mut self, only_stale: bool) {
        let Some(split) = self.split.as_mut() else {
            return;
        };
        let mut jobs = Vec::new();
        for group in &mut split.groups {
            if group.hunks.is_empty() || (only_stale && !group.stale) {
                continue;
            }
            self.next_id += 1;
            group.gen_id = self.next_id;
            group.generating = true;
            group.stale = false;
            let diff = split
                .staged
                .display_for(&group.hunks, |file| git::is_excluded_from_diff(&file.path));
            jobs.push((group.gen_id, diff, split.staged.files_for(&group.hunks)));
        }
        for (gen_id, diff, files) in jobs {
            let tx = self.tx.clone();
            let config = self.input.config.clone();
            let context = self.input.context.clone();
            let gitmoji = self.input.full_gitmoji_spec;
            let extra_args = self.input.extra_args.clone();
            tokio::spawn(async move {
                let result = generator::generate_commit_message(
                    &config, &diff, gitmoji, &context, &files, None,
                )
                .await
                .map(|message| {
                    crate::commands::commit::apply_message_template(&config, &extra_args, &message)
                })
                .map_err(|error| error.to_string());
                let _ = tx.send(AppEvent::GroupMessage {
                    group_gen: gen_id,
                    result,
                });
            });
        }
    }

    pub fn handle_app_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Progress(id, progress) => {
                if let Gen::Running {
                    id: current, stage, ..
                } = &mut self.generation
                    && *current == id
                {
                    *stage = match progress {
                        GenerationProgress::Splitting => "Preparing diff".to_owned(),
                        GenerationProgress::Chunk {
                            current: 1,
                            total: 1,
                        } => "Writing commit message".to_owned(),
                        GenerationProgress::Chunk { current, total } => {
                            format!("Summarising chunk {current}/{total}")
                        }
                        GenerationProgress::Synthesizing => "Synthesising".to_owned(),
                    };
                }
            }
            AppEvent::Delta(id, delta) => {
                if matches!(&self.generation, Gen::Running { id: current, .. } if *current == id) {
                    self.streamed.push_str(&delta);
                    self.message.set_text(&self.streamed);
                }
            }
            AppEvent::MessageDone(id, result) => {
                if !matches!(&self.generation, Gen::Running { id: current, .. } if *current == id) {
                    return;
                }
                match result {
                    Ok(message) => {
                        self.message.set_text(&message);
                        self.generation = Gen::Idle;
                        self.needs_clear = true;
                    }
                    Err(error) => {
                        self.generation = Gen::Failed(error);
                    }
                }
            }
            AppEvent::PlanDone(id, result) => {
                let Some(split) = self.split.as_mut() else {
                    return;
                };
                if split.plan_id != id {
                    return;
                }
                match result {
                    Ok(groups) => {
                        split.groups = groups
                            .into_iter()
                            .map(|group| GroupState {
                                title: group.title,
                                rationale: group.rationale,
                                hunks: group.hunks,
                                message: String::new(),
                                generating: false,
                                stale: true,
                                gen_id: 0,
                            })
                            .collect();
                        split.status = SplitStatus::Ready;
                        split.appeared = Instant::now();
                        self.generate_group_messages(true);
                    }
                    Err(error) => split.status = SplitStatus::Failed(error),
                }
            }
            AppEvent::GroupMessage { group_gen, result } => {
                let Some(split) = self.split.as_mut() else {
                    return;
                };
                if let Some(group) = split.groups.iter_mut().find(|g| g.gen_id == group_gen) {
                    group.generating = false;
                    match result {
                        Ok(message) => group.message = message,
                        Err(error) => {
                            group.message.clear();
                            self.toast = Some((
                                format!("Couldn't draft “{}”: {error}", group.title),
                                true,
                                Instant::now(),
                            ));
                        }
                    }
                }
            }
        }
    }

    /// Periodic work: expire toasts, regenerate stale split groups.
    pub fn tick(&mut self) {
        if self
            .toast
            .as_ref()
            .is_some_and(|(_, _, at)| at.elapsed() > TOAST_DURATION)
        {
            self.toast = None;
        }
        let due = self
            .split
            .as_ref()
            .and_then(|split| split.dirty_at)
            .is_some_and(|at| at.elapsed() > REGENERATE_DEBOUNCE);
        if due {
            if let Some(split) = self.split.as_mut() {
                split.dirty_at = None;
            }
            self.generate_group_messages(true);
        }
    }

    // -----------------------------------------------------------------
    // Input
    // -----------------------------------------------------------------

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.cancel_generation();
            self.outcome = Some(Outcome::Quit);
            return;
        }
        if self.help {
            self.help = false;
            return;
        }
        if self.palette_overlay.is_some() {
            self.handle_palette_key(key);
            return;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('k') {
            self.open_palette();
            return;
        }
        match self.screen {
            Screen::Commit => self.handle_commit_key(key),
            Screen::Split => self.handle_split_key(key),
            Screen::Settings => self.handle_settings_key(key),
        }
    }

    fn open_palette(&mut self) {
        self.palette_overlay = Some(PaletteState {
            query: String::new(),
            selected: 0,
        });
    }

    fn handle_palette_key(&mut self, key: KeyEvent) {
        let Some(state) = self.palette_overlay.as_mut() else {
            return;
        };
        match key.code {
            KeyCode::Esc => self.palette_overlay = None,
            KeyCode::Up => state.selected = state.selected.saturating_sub(1),
            KeyCode::Down => {
                state.selected = (state.selected + 1).min(state.matches().len().saturating_sub(1))
            }
            KeyCode::Backspace => {
                state.query.pop();
                state.selected = 0;
            }
            KeyCode::Char(ch) => {
                state.query.push(ch);
                state.selected = 0;
            }
            KeyCode::Enter => {
                let action = state.matches().get(state.selected).copied();
                self.palette_overlay = None;
                if let Some(action) = action {
                    if self.screen != Screen::Commit {
                        self.screen = Screen::Commit;
                        self.split = None;
                        self.settings = None;
                    }
                    self.run_action(action);
                }
            }
            _ => {}
        }
    }

    pub fn run_action(&mut self, action: Action) {
        match action {
            Action::Generate => self.start_generation(),
            Action::Edit => {
                if !self.is_generating() {
                    self.focus = Focus::Message;
                }
            }
            Action::ToggleStage => self.toggle_stage(),
            Action::StageAll => self.stage_all(true),
            Action::UnstageAll => self.stage_all(false),
            Action::Split => self.start_split(),
            Action::Commit => self.commit(),
            Action::Theme => self.next_theme(),
            Action::Settings => self.open_settings(false),
            Action::Help => self.help = true,
            Action::Quit => {
                self.cancel_generation();
                self.outcome = Some(Outcome::Quit);
            }
        }
    }

    fn next_theme(&mut self) {
        let current = self
            .theme_names
            .iter()
            .position(|name| {
                crate::map::theme::load_theme(name).is_ok_and(|t| t.name == self.palette.name)
            })
            .unwrap_or(0);
        let next = &self.theme_names[(current + 1) % self.theme_names.len()];
        self.palette = super::theme::palette(next);
        self.needs_clear = true;
        self.toast(format!("Theme: {}", self.palette.name), false);
    }

    fn commit(&mut self) {
        if self.is_generating() {
            self.toast(
                "Wait for the message to finish, or press esc to cancel",
                true,
            );
            return;
        }
        if self.staged_files.is_empty() {
            self.toast("Nothing staged to commit", true);
            return;
        }
        if self.message.is_empty() {
            self.toast("Write or generate a message first (g)", true);
            return;
        }
        self.outcome = Some(Outcome::Commit {
            message: self.message.text(),
            files: self.staged_files.clone(),
        });
    }

    fn handle_commit_key(&mut self, key: KeyEvent) {
        if self.focus == Focus::Message {
            match key.code {
                KeyCode::Esc | KeyCode::Tab => self.focus = Focus::Tree,
                KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.focus = Focus::Tree;
                    self.commit();
                }
                _ => {
                    self.message.handle_key(key);
                }
            }
            return;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::PageDown | KeyCode::Char('J') => {
                self.diff_scroll = self.diff_scroll.saturating_add(self.diff_height / 2)
            }
            KeyCode::PageUp | KeyCode::Char('K') => {
                self.diff_scroll = self.diff_scroll.saturating_sub(self.diff_height / 2)
            }
            KeyCode::Char(' ') => self.run_action(Action::ToggleStage),
            KeyCode::Char('a') => self.run_action(Action::StageAll),
            KeyCode::Char('u') => self.run_action(Action::UnstageAll),
            KeyCode::Char('g') | KeyCode::Char('r') => self.run_action(Action::Generate),
            KeyCode::Char('e') | KeyCode::Tab => self.run_action(Action::Edit),
            KeyCode::Char('s') => self.run_action(Action::Split),
            KeyCode::Char('c') => self.run_action(Action::Commit),
            KeyCode::Char('t') => self.run_action(Action::Theme),
            KeyCode::Char(',') => self.run_action(Action::Settings),
            KeyCode::Char('?') => self.run_action(Action::Help),
            KeyCode::Char(':') => self.open_palette(),
            KeyCode::Char('q') => self.run_action(Action::Quit),
            KeyCode::Esc if self.is_generating() => {
                self.cancel_generation();
                self.toast("Generation cancelled", false);
            }
            _ => {}
        }
    }

    fn handle_split_key(&mut self, key: KeyEvent) {
        let Some(split) = self.split.as_mut() else {
            self.screen = Screen::Commit;
            return;
        };
        if let Some(editor) = split.editing.as_mut() {
            match key.code {
                KeyCode::Esc => {
                    let text = editor.text();
                    if let Some(group) = split.groups.get_mut(split.col) {
                        group.message = text;
                        group.stale = false;
                    }
                    split.editing = None;
                }
                _ => {
                    editor.handle_key(key);
                }
            }
            return;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.split = None;
                self.screen = Screen::Commit;
            }
            KeyCode::Char('?') => self.help = true,
            KeyCode::Up | KeyCode::Char('k') => split.row = split.row.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                let len = split.groups.get(split.col).map_or(0, |g| g.hunks.len());
                split.row = (split.row + 1).min(len.saturating_sub(1));
            }
            KeyCode::Tab | KeyCode::Char('l') => {
                if !split.groups.is_empty() {
                    split.col = (split.col + 1) % split.groups.len();
                    split.row = 0;
                }
            }
            KeyCode::BackTab | KeyCode::Char('h') => {
                if !split.groups.is_empty() {
                    split.col = (split.col + split.groups.len() - 1) % split.groups.len();
                    split.row = 0;
                }
            }
            KeyCode::Right | KeyCode::Char('L') => {
                let target = split.col + 1;
                move_hunk(split, target);
            }
            KeyCode::Left | KeyCode::Char('H') => {
                if split.col > 0 {
                    let target = split.col - 1;
                    move_hunk(split, target);
                }
            }
            KeyCode::Char('n') => {
                if split.groups.len() < MAX_GROUPS && matches!(split.status, SplitStatus::Ready) {
                    split.groups.push(GroupState {
                        title: format!("Commit {}", split.groups.len() + 1),
                        rationale: "New group".to_owned(),
                        hunks: Vec::new(),
                        message: String::new(),
                        generating: false,
                        stale: false,
                        gen_id: 0,
                    });
                }
            }
            KeyCode::Char('m') => {
                if split.groups.len() > 1 {
                    let from = split.col;
                    let into = if from == 0 { 1 } else { from - 1 };
                    let moved = std::mem::take(&mut split.groups[from].hunks);
                    split.groups[into].hunks.extend(moved);
                    sort_hunks(&split.staged, &mut split.groups[into].hunks);
                    split.groups[into].stale = true;
                    split.groups.remove(from);
                    split.col = if from == 0 { 0 } else { from - 1 };
                    split.row = 0;
                    split.dirty_at = Some(Instant::now());
                }
            }
            KeyCode::Char('r') => {
                if let Some(group) = split.groups.get_mut(split.col) {
                    group.stale = true;
                }
                self.generate_group_messages(true);
            }
            KeyCode::Char('e') => {
                if let Some(group) = split.groups.get(split.col) {
                    split.editing = Some(TextEditor::new(&group.message));
                }
            }
            KeyCode::Enter => self.commit_split(),
            _ => {}
        }
    }

    pub fn open_settings(&mut self, first_run: bool) {
        self.cancel_generation();
        self.settings = Some(SettingsState::new(self.config_path.clone(), first_run));
        self.screen = Screen::Settings;
        self.needs_clear = true;
    }

    fn close_settings(&mut self) {
        let first_run = self.settings.as_ref().is_some_and(|s| s.first_run);
        if first_run
            && self.input.config.provider_needs_api_key()
            && self.input.config.api_key.is_none()
        {
            self.toast(
                "Add an API key first, or pick a provider that doesn't need one (q quits)",
                true,
            );
            return;
        }
        self.settings = None;
        self.screen = Screen::Commit;
        self.needs_clear = true;
        if first_run && !self.staged_files.is_empty() && self.message.is_empty() {
            self.start_generation();
        }
    }

    fn save_setting(&mut self, key: &str, value: &str) {
        let path = self.config_path.clone();
        match settings::save(&path, key, value) {
            Ok(config) => {
                let theme_changed = config.theme != self.input.config.theme
                    || config.icons != self.input.config.icons;
                self.input.config = config;
                if theme_changed {
                    self.palette = super::theme::palette(&self.input.config.theme);
                    self.needs_clear = true;
                }
                let note = if settings::env_override(key) {
                    format!("Saved {key}, but the {key} environment variable still wins")
                } else {
                    format!("Saved {key}")
                };
                self.toast(note, settings::env_override(key));
            }
            Err(error) => self.toast(format!("Couldn't save {key}: {error}"), true),
        }
    }

    fn handle_settings_key(&mut self, key: KeyEvent) {
        let Some(state) = self.settings.as_mut() else {
            self.screen = Screen::Commit;
            return;
        };
        let field = state.field();

        if let Some(editor) = state.editing.as_mut() {
            match key.code {
                KeyCode::Esc => state.editing = None,
                KeyCode::Enter => {
                    let value = editor.text();
                    state.editing = None;
                    self.save_setting(field.key, value.trim());
                }
                _ => {
                    editor.handle_key(key);
                }
            }
            return;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char(',') => {
                if key.code == KeyCode::Char('q') && state.first_run {
                    self.outcome = Some(Outcome::Quit);
                } else {
                    self.close_settings();
                }
            }
            KeyCode::Char('?') => self.help = true,
            KeyCode::Up | KeyCode::Char('k') => {
                state.selected = state.selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                state.selected = (state.selected + 1).min(settings::FIELDS.len() - 1);
            }
            KeyCode::Left | KeyCode::Right | KeyCode::Char('h') | KeyCode::Char('l')
                if field.kind == FieldKind::Choice =>
            {
                let step = if matches!(key.code, KeyCode::Left | KeyCode::Char('h')) {
                    -1
                } else {
                    1
                };
                if let Some(value) = settings::cycle(field.key, &self.input.config, step) {
                    self.save_setting(field.key, &value);
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') => match field.kind {
                FieldKind::Bool => {
                    let current = self.input.config.get_key(field.key).unwrap_or_default();
                    let next = if current == "true" { "false" } else { "true" };
                    self.save_setting(field.key, next);
                }
                FieldKind::Choice if field.key != "AIC_MODEL" => {
                    if let Some(value) = settings::cycle(field.key, &self.input.config, 1) {
                        self.save_setting(field.key, &value);
                    }
                }
                FieldKind::Secret => state.editing = Some(TextEditor::new("")),
                _ => {
                    let current = self.input.config.get_key(field.key).unwrap_or_default();
                    state.editing = Some(TextEditor::new(&current));
                }
            },
            KeyCode::Backspace | KeyCode::Delete
                if matches!(field.kind, FieldKind::Text | FieldKind::Secret) =>
            {
                self.save_setting(field.key, "");
            }
            _ => {}
        }
    }

    fn commit_split(&mut self) {
        let Some(split) = self.split.as_ref() else {
            return;
        };
        if !matches!(split.status, SplitStatus::Ready) {
            return;
        }
        if split
            .groups
            .iter()
            .any(|g| g.generating || (g.stale && !g.hunks.is_empty()))
        {
            self.toast("Some messages are still being written", true);
            return;
        }
        let groups = split
            .groups
            .iter()
            .filter(|group| !group.hunks.is_empty())
            .collect::<Vec<_>>();
        if groups.iter().any(|group| group.message.trim().is_empty()) {
            self.toast(
                "Every commit needs a message (r to regenerate, e to edit)",
                true,
            );
            return;
        }
        let groups = groups
            .into_iter()
            .map(|group| {
                (
                    SplitPlanGroup {
                        title: group.title.clone(),
                        rationale: group.rationale.clone(),
                        files: split.staged.files_for(&group.hunks),
                        hunks: group.hunks.clone(),
                    },
                    group.message.clone(),
                )
            })
            .collect();
        self.outcome = Some(Outcome::Split {
            staged: split.staged.clone(),
            groups,
        });
    }

    pub fn handle_mouse(&mut self, mouse: MouseEvent) {
        let position = Position::new(mouse.column, mouse.row);
        let hit = self
            .hits
            .iter()
            .rev()
            .find(|(rect, _)| rect.contains(position))
            .map(|(_, hit)| *hit);
        match mouse.kind {
            MouseEventKind::ScrollDown if hit == Some(Hit::Diff) => {
                self.diff_scroll = self.diff_scroll.saturating_add(3)
            }
            MouseEventKind::ScrollUp if hit == Some(Hit::Diff) => {
                self.diff_scroll = self.diff_scroll.saturating_sub(3)
            }
            MouseEventKind::ScrollDown if self.screen == Screen::Commit => self.move_selection(1),
            MouseEventKind::ScrollUp if self.screen == Screen::Commit => self.move_selection(-1),
            MouseEventKind::Down(MouseButton::Left) => match hit {
                Some(Hit::TreeRow(index)) => {
                    if self.selected == index {
                        self.toggle_stage();
                    } else {
                        self.selected = index;
                        self.diff_scroll = 0;
                        self.focus = Focus::Tree;
                    }
                }
                Some(Hit::Message) if !self.is_generating() => self.focus = Focus::Message,
                Some(Hit::SplitCard(col, row)) => {
                    if let Some(split) = self.split.as_mut() {
                        split.col = col;
                        split.row = row;
                        split.drag = Some((col, row));
                    }
                }
                Some(Hit::SplitColumn(col)) => {
                    if let Some(split) = self.split.as_mut() {
                        split.col = col;
                        split.row = 0;
                    }
                }
                _ => {}
            },
            MouseEventKind::Up(MouseButton::Left) => {
                if let Some(split) = self.split.as_mut()
                    && let Some((from_col, from_row)) = split.drag.take()
                {
                    let target = match hit {
                        Some(Hit::SplitCard(col, _)) | Some(Hit::SplitColumn(col)) => Some(col),
                        _ => None,
                    };
                    if let Some(target) = target
                        && target != from_col
                    {
                        split.col = from_col;
                        split.row = from_row;
                        move_hunk(split, target);
                    }
                }
            }
            _ => {}
        }
    }

    // -----------------------------------------------------------------
    // Queries used by the view
    // -----------------------------------------------------------------

    pub fn selected_row(&self) -> Option<&RowKind> {
        self.rows.get(self.selected)
    }

    /// The diff shown for the current selection: (path, hunks, selected hunk index).
    pub fn selected_diff(&self) -> Option<(String, Vec<String>, Option<usize>)> {
        let row = self.selected_row()?;
        let (path, staged, hunk) = match row {
            RowKind::File { path, staged, .. } => (path.clone(), *staged, None),
            RowKind::Hunk {
                path, id, staged, ..
            } => (path.clone(), *staged, Some(id.clone())),
            RowKind::Untracked { path } => {
                let content = git::repo_root()
                    .ok()
                    .and_then(|root| std::fs::read(root.join(path)).ok())
                    .map(|bytes| {
                        if bytes.contains(&0) {
                            "@@ new binary file @@\n".to_owned()
                        } else {
                            let text = String::from_utf8_lossy(&bytes);
                            let lines: Vec<&str> = text.lines().take(400).collect();
                            format!(
                                "@@ -0,0 +1,{} @@ new file\n{}\n",
                                lines.len(),
                                lines
                                    .iter()
                                    .map(|line| format!("+{line}"))
                                    .collect::<Vec<_>>()
                                    .join("\n")
                            )
                        }
                    })
                    .unwrap_or_default();
                return Some((path.clone(), vec![content], None));
            }
            _ => return None,
        };
        let source = if staged { &self.staged } else { &self.unstaged };
        let file = source.file(&path)?;
        let texts = hunk_texts(file);
        let selected = hunk.and_then(|id| {
            file.hunk_ids()
                .iter()
                .position(|candidate| *candidate == id)
        });
        Some((path, texts, selected))
    }

    pub fn split_hunk_text(&self, id: &str) -> Option<String> {
        let split = self.split.as_ref()?;
        let file = split.staged.file(hunk_path(id))?;
        let index = file
            .hunk_ids()
            .iter()
            .position(|candidate| candidate == id)?;
        hunk_texts(file).into_iter().nth(index)
    }

    pub fn shared_files(&self) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::new();
        if let Some(split) = &self.split {
            for group in &split.groups {
                let mut seen = std::collections::BTreeSet::new();
                for id in &group.hunks {
                    if seen.insert(hunk_path(id).to_owned()) {
                        *counts.entry(hunk_path(id).to_owned()).or_insert(0) += 1;
                    }
                }
            }
        }
        counts.retain(|_, count| *count > 1);
        counts
    }
}

/// Text of each hunk, or one pseudo-hunk for atomic files.
pub fn hunk_texts(file: &git::FileDiff) -> Vec<String> {
    if file.atomic || file.hunks.is_empty() {
        let body = file
            .raw
            .lines()
            .skip_while(|line| !line.starts_with("@@"))
            .collect::<Vec<_>>()
            .join("\n");
        let summary = file
            .header
            .lines()
            .find(|line| {
                line.starts_with("new file")
                    || line.starts_with("deleted file")
                    || line.starts_with("rename")
                    || line.starts_with("old mode")
                    || line.starts_with("Binary files")
            })
            .unwrap_or("change");
        if body.is_empty() {
            vec![format!("@@ {summary} @@\n")]
        } else {
            vec![body]
        }
    } else {
        file.hunks.iter().map(|hunk| hunk.text.clone()).collect()
    }
}

fn push_hunk_rows(rows: &mut Vec<RowKind>, file: &git::FileDiff, staged: bool) {
    let texts = hunk_texts(file);
    for (id, text) in file.hunk_ids().into_iter().zip(texts) {
        let label = if file.atomic {
            file.header
                .lines()
                .find_map(|line| {
                    if line.starts_with("new file") {
                        Some("new file")
                    } else if line.starts_with("deleted file") {
                        Some("deleted")
                    } else if line.starts_with("rename from") {
                        Some("renamed")
                    } else if line.starts_with("old mode") {
                        Some("mode change")
                    } else if line.starts_with("Binary files") {
                        Some("binary")
                    } else {
                        None
                    }
                })
                .unwrap_or("whole file")
                .to_owned()
        } else {
            super::highlight::hunk_label(&text)
        };
        rows.push(RowKind::Hunk {
            path: file.path.clone(),
            id,
            staged,
            label,
            stats: super::highlight::hunk_stats(&text),
        });
    }
}

fn row_path(row: &RowKind) -> Option<&str> {
    match row {
        RowKind::File { path, .. } | RowKind::Hunk { path, .. } | RowKind::Untracked { path } => {
            Some(path)
        }
        _ => None,
    }
}

fn plural(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

fn sort_hunks(staged: &StagedDiff, hunks: &mut [String]) {
    let order = staged.hunk_ids();
    hunks.sort_by_key(|id| order.iter().position(|known| known == id));
}

/// Move the selected hunk into group `target`, creating a new group when
/// moving right past the last one.
pub fn move_hunk(split: &mut SplitState, target: usize) {
    if !matches!(split.status, SplitStatus::Ready) {
        return;
    }
    if target >= split.groups.len() {
        if split.groups.len() >= MAX_GROUPS {
            return;
        }
        split.groups.push(GroupState {
            title: format!("Commit {}", split.groups.len() + 1),
            rationale: "New group".to_owned(),
            hunks: Vec::new(),
            message: String::new(),
            generating: false,
            stale: false,
            gen_id: 0,
        });
    }
    let target = target.min(split.groups.len() - 1);
    if target == split.col || split.row >= split.groups[split.col].hunks.len() {
        return;
    }
    let id = split.groups[split.col].hunks.remove(split.row);
    split.groups[target].hunks.push(id.clone());
    sort_hunks(&split.staged, &mut split.groups[target].hunks);
    split.groups[split.col].stale = true;
    split.groups[target].stale = true;
    split.dirty_at = Some(Instant::now());
    split.col = target;
    split.row = split.groups[target]
        .hunks
        .iter()
        .position(|candidate| *candidate == id)
        .unwrap_or(0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::diff::parse_diff;

    fn split_state() -> SplitState {
        let staged = parse_diff(
            "diff --git a/a.rs b/a.rs\nindex 1..2 100644\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-a\n+A\n@@ -9 +9 @@\n-b\n+B\ndiff --git a/c.md b/c.md\nindex 3..4 100644\n--- a/c.md\n+++ b/c.md\n@@ -1 +1 @@\n-c\n+C\n",
        );
        let group = |hunks: &[&str]| GroupState {
            title: "g".to_owned(),
            rationale: "r".to_owned(),
            hunks: hunks.iter().map(|h| (*h).to_owned()).collect(),
            message: "m".to_owned(),
            generating: false,
            stale: false,
            gen_id: 0,
        };
        SplitState {
            staged,
            groups: vec![group(&["a.rs#1", "a.rs#2"]), group(&["c.md#1"])],
            col: 0,
            row: 1,
            appeared: Instant::now(),
            dirty_at: None,
            status: SplitStatus::Ready,
            plan_id: 1,
            editing: None,
            drag: None,
        }
    }

    #[test]
    fn moving_a_hunk_keeps_diff_order_and_marks_groups_stale() {
        let mut split = split_state();
        move_hunk(&mut split, 1);
        assert_eq!(split.groups[0].hunks, vec!["a.rs#1"]);
        assert_eq!(split.groups[1].hunks, vec!["a.rs#2", "c.md#1"]);
        assert!(split.groups[0].stale && split.groups[1].stale);
        assert_eq!((split.col, split.row), (1, 0));
        assert!(split.dirty_at.is_some());
    }

    #[test]
    fn moving_past_the_last_group_creates_one() {
        let mut split = split_state();
        split.col = 1;
        split.row = 0;
        move_hunk(&mut split, 2);
        assert_eq!(split.groups.len(), 3);
        assert_eq!(split.groups[2].hunks, vec!["c.md#1"]);
    }

    #[test]
    fn palette_matches_fuzzily() {
        let state = PaletteState {
            query: "cmt".to_owned(),
            selected: 0,
        };
        assert!(state.matches().contains(&Action::Commit));
        assert!(fuzzy_match("split into several commits", "splt"));
        assert!(!fuzzy_match("commit", "xyz"));
    }
}

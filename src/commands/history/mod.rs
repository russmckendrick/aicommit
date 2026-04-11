use anyhow::Result;

use crate::history_store;

mod format;
mod interactive;
mod render;

pub fn run(
    count: usize,
    kind: Option<String>,
    include_all: bool,
    verbose: bool,
    interactive: bool,
    non_interactive: bool,
) -> Result<()> {
    let result = history_store::recent_entries(count, kind.as_deref())?;
    let interactive = interactive::should_use_interactive(interactive, non_interactive);

    if interactive {
        return interactive::run_interactive(&result, include_all);
    }

    render::render_history(&result, kind.as_deref(), include_all, verbose)
}

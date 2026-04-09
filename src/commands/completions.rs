use clap_complete::{Shell, generate};

use crate::cli;

pub fn run(shell: Shell) {
    let mut cmd = cli::command();
    cmd.build();
    generate(shell, &mut cmd, "aic", &mut std::io::stdout());
}

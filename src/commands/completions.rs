use clap::CommandFactory;
use clap_complete::{Shell, generate};

use crate::cli::Cli;

pub fn run(shell: Shell) {
    let mut cmd = Cli::command();
    cmd.build();
    generate(shell, &mut cmd, "aic", &mut std::io::stdout());
}

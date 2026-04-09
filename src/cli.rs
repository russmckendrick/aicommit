use anyhow::Result;
use clap::{Args, CommandFactory, FromArgMatches, Parser, Subcommand};
use clap_complete::Shell;

use crate::commands;

#[derive(Debug, Parser)]
#[command(name = "aic", version)]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[arg(short = 'p', long, global = true)]
    provider: Option<String>,

    #[arg(long = "fgm")]
    full_gitmoji_spec: bool,

    #[arg(short = 'c', long, default_value = "")]
    context: String,

    #[arg(short = 'y', long)]
    yes: bool,

    #[arg(short = 'd', long)]
    dry_run: bool,

    #[arg(long)]
    amend: bool,

    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    git_args: Vec<String>,
}

#[derive(Debug, Subcommand)]
enum Command {
    Config(ConfigCommand),
    Setup,
    Models(ModelsCommand),
    Hook(HookCommand),
    #[command(name = "hookrun", hide = true)]
    HookRun(HookRunCommand),
    Completions(CompletionsCommand),
    Review(ReviewCommand),
    History(HistoryCommand),
    Log(LogCommand),
}

#[derive(Debug, Args)]
struct ConfigCommand {
    #[command(subcommand)]
    mode: ConfigMode,
}

#[derive(Debug, Subcommand)]
enum ConfigMode {
    Set { key_values: Vec<String> },
    Get { keys: Vec<String> },
    Describe { keys: Vec<String> },
}

#[derive(Debug, Args)]
struct ModelsCommand {
    #[arg(short, long)]
    refresh: bool,
}

#[derive(Debug, Args)]
struct HookCommand {
    #[command(subcommand)]
    mode: HookMode,
}

#[derive(Debug, Subcommand)]
enum HookMode {
    Set,
    Unset,
}

#[derive(Debug, Args)]
struct HookRunCommand {
    message_file: String,
    commit_source: Option<String>,
}

#[derive(Debug, Args)]
struct CompletionsCommand {
    shell: Shell,
}

#[derive(Debug, Args)]
struct ReviewCommand {
    #[arg(short = 'c', long, default_value = "")]
    context: String,
}

#[derive(Debug, Args)]
struct HistoryCommand {
    #[arg(short = 'n', long, default_value = "10")]
    count: usize,

    #[arg(short = 'k', long)]
    kind: Option<String>,

    #[arg(long)]
    all: bool,

    #[arg(long)]
    verbose: bool,

    #[arg(short = 'i', long = "interactive", conflicts_with = "non_interactive")]
    interactive: bool,

    #[arg(long = "non-interactive", conflicts_with = "interactive")]
    non_interactive: bool,
}

#[derive(Debug, Args)]
struct LogCommand {
    #[arg(short = 'n', long, default_value = "5")]
    count: usize,

    #[arg(short = 'y', long)]
    yes: bool,
}

pub fn command() -> clap::Command {
    crate::cli_text::command(Cli::command())
}

pub async fn run() -> Result<()> {
    let mut matches = command().get_matches();
    let cli = Cli::from_arg_matches_mut(&mut matches)?;

    match cli.command {
        Some(Command::Config(command)) => match command.mode {
            ConfigMode::Set { key_values } => commands::config::set(key_values),
            ConfigMode::Get { keys } => commands::config::get(keys),
            ConfigMode::Describe { keys } => commands::config::describe(keys),
        },
        Some(Command::Setup) => commands::setup::run().await,
        Some(Command::Models(command)) => {
            commands::models::run(cli.provider, command.refresh).await
        }
        Some(Command::Hook(command)) => match command.mode {
            HookMode::Set => commands::hook::set(),
            HookMode::Unset => commands::hook::unset(),
        },
        Some(Command::HookRun(command)) => {
            commands::hook::run_hook(command.message_file, command.commit_source).await
        }
        Some(Command::Completions(command)) => {
            commands::completions::run(command.shell);
            Ok(())
        }
        Some(Command::Review(command)) => {
            commands::review::run(command.context, cli.provider).await
        }
        Some(Command::History(command)) => commands::history::run(
            command.count,
            command.kind,
            command.all,
            command.verbose,
            command.interactive,
            command.non_interactive,
        ),
        Some(Command::Log(command)) => {
            commands::log::run(command.count, command.yes, cli.provider).await
        }
        None => {
            commands::commit::run(
                cli.git_args,
                cli.context,
                cli.full_gitmoji_spec,
                cli.yes,
                cli.dry_run,
                cli.amend,
                cli.provider,
            )
            .await
        }
    }
}

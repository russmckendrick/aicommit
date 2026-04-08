use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use clap_complete::Shell;

use crate::commands;

#[derive(Debug, Parser)]
#[command(name = "aic", version, about = "AI-assisted Git commit messages")]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[arg(long = "fgm", help = "Use the full GitMoji prompt")]
    full_gitmoji_spec: bool,

    #[arg(
        short = 'c',
        long,
        default_value = "",
        help = "Additional commit context"
    )]
    context: String,

    #[arg(short = 'y', long, help = "Skip commit confirmation")]
    yes: bool,

    #[arg(
        short = 'd',
        long,
        help = "Generate and print the message without committing"
    )]
    dry_run: bool,

    #[arg(long, help = "Regenerate and amend the last commit message")]
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
    #[arg(short, long)]
    provider: Option<String>,
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
#[command(about = "Generate shell completions")]
struct CompletionsCommand {
    #[arg(help = "Shell to generate completions for")]
    shell: Shell,
}

#[derive(Debug, Args)]
#[command(
    about = "Review staged changes for bugs, security, and style issues",
    long_about = "Review staged changes for bugs, security, and style issues.\n\n\
        Only the staged diff (git diff --staged) is reviewed. Unstaged changes are ignored.\n\n\
        Examples:\n  \
        aic review\n  \
        aic review -c 'focus on security'\n  \
        aic review --context 'this is a database migration'"
)]
struct ReviewCommand {
    #[arg(
        short = 'c',
        long,
        default_value = "",
        help = "Focus the review on a specific concern (e.g., 'focus on security')"
    )]
    context: String,
}

pub async fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Config(command)) => match command.mode {
            ConfigMode::Set { key_values } => commands::config::set(key_values),
            ConfigMode::Get { keys } => commands::config::get(keys),
            ConfigMode::Describe { keys } => commands::config::describe(keys),
        },
        Some(Command::Setup) => commands::setup::run().await,
        Some(Command::Models(command)) => {
            commands::models::run(command.provider, command.refresh).await
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
        Some(Command::Review(command)) => commands::review::run(command.context).await,
        None => {
            commands::commit::run(
                cli.git_args,
                cli.context,
                cli.full_gitmoji_spec,
                cli.yes,
                cli.dry_run,
                cli.amend,
            )
            .await
        }
    }
}

use std::{collections::BTreeMap, sync::OnceLock};

use clap::{Arg, Command};
use serde::Deserialize;

static CLI_TEXT: OnceLock<CliText> = OnceLock::new();

#[derive(Debug, Deserialize)]
struct CliText {
    root: CommandText,
    #[serde(default)]
    config_keys: BTreeMap<String, String>,
}

#[derive(Debug, Default, Deserialize)]
struct CommandText {
    about: Option<String>,
    long_about: Option<String>,
    #[serde(default)]
    args: BTreeMap<String, ArgText>,
    #[serde(default)]
    commands: BTreeMap<String, CommandText>,
}

#[derive(Debug, Default, Deserialize)]
struct ArgText {
    help: Option<String>,
    long_help: Option<String>,
}

pub fn command(command: Command) -> Command {
    apply_command_text(command, &cli_text().root)
}

pub fn config_description(key: &str) -> Option<&'static str> {
    cli_text().config_keys.get(key).map(String::as_str)
}

fn cli_text() -> &'static CliText {
    CLI_TEXT.get_or_init(|| {
        toml_edit::de::from_str(include_str!("cli_help.toml"))
            .expect("bundled CLI help metadata should parse")
    })
}

fn apply_command_text(command: Command, text: &CommandText) -> Command {
    let command = if let Some(about) = &text.about {
        command.about(about.clone())
    } else {
        command
    };
    let command = if let Some(long_about) = &text.long_about {
        command.long_about(long_about.clone())
    } else {
        command
    };

    let command = command.mut_args(|arg| match text.args.get(arg.get_id().as_str()) {
        Some(arg_text) => apply_arg_text(arg, arg_text),
        None => arg,
    });

    command.mut_subcommands(
        |subcommand| match text.commands.get(subcommand.get_name()) {
            Some(sub_text) => apply_command_text(subcommand, sub_text),
            None => subcommand,
        },
    )
}

fn apply_arg_text(arg: Arg, text: &ArgText) -> Arg {
    let arg = if let Some(help) = &text.help {
        arg.help(help.clone())
    } else {
        arg
    };

    if let Some(long_help) = &text.long_help {
        arg.long_help(long_help.clone())
    } else {
        arg
    }
}

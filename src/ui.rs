use std::fmt::Display;

use anyhow::{Error, Result};
use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use inquire::{Confirm, InquireError, MultiSelect, Select, Text};

pub fn info(message: impl AsRef<str>) {
    println!("{}", message.as_ref());
}

pub fn success(message: impl AsRef<str>) {
    println!("{} {}", style("✔").green(), style(message.as_ref()).green());
}

pub fn warn(message: impl AsRef<str>) {
    eprintln!(
        "{} {}",
        style("warning:").yellow(),
        style(message.as_ref()).yellow()
    );
}

pub fn section(title: impl AsRef<str>) {
    println!("{} {}", style("◇").cyan(), style(title.as_ref()).bold());
}

pub fn blank_line() {
    println!();
}

pub fn bullet(message: impl AsRef<str>) {
    println!("  {} {}", style("•").cyan().dim(), message.as_ref());
}

pub fn secondary(message: impl AsRef<str>) {
    for line in message.as_ref().lines() {
        println!("  {}", style(line).dim());
    }
}

pub fn headline(message: impl AsRef<str>) {
    println!("  {}", style(message.as_ref()).bold());
}

pub fn commit_message(message: impl AsRef<str>) {
    for (index, line) in message.as_ref().lines().enumerate() {
        if index == 0 {
            println!("  {}", style(line).bold());
        } else if line.trim().is_empty() {
            println!();
        } else {
            println!("  {line}");
        }
    }
}

pub fn spinner(message: impl Into<String>) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("{spinner:.cyan} {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_spinner()),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    pb.set_message(message.into());
    pb
}

pub fn confirm(message: &str, default: bool) -> Result<bool> {
    Ok(Confirm::new(message).with_default(default).prompt()?)
}

pub fn select<T>(message: &str, options: Vec<T>) -> Result<T>
where
    T: Clone + Display,
{
    Ok(Select::new(message, options).prompt()?)
}

pub fn multiselect(message: &str, options: Vec<String>) -> Result<Vec<String>> {
    Ok(MultiSelect::new(message, options).prompt()?)
}

pub fn markdown(text: &str) {
    let skin = termimad::MadSkin::default_dark();
    skin.print_text(text);
}

pub fn text(message: &str, initial: Option<&str>) -> Result<String> {
    let prompt = Text::new(message);
    let prompt = if let Some(initial) = initial {
        prompt.with_initial_value(initial)
    } else {
        prompt
    };
    Ok(prompt.prompt()?)
}

pub fn is_prompt_cancelled(error: &Error) -> bool {
    matches!(
        error.downcast_ref::<InquireError>(),
        Some(InquireError::OperationCanceled | InquireError::OperationInterrupted)
    )
}

use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use inquire::{Confirm, MultiSelect, Select, Text};

pub fn info(message: impl AsRef<str>) {
    println!("{}", message.as_ref());
}

pub fn success(message: impl AsRef<str>) {
    println!("✔ {}", message.as_ref());
}

pub fn warn(message: impl AsRef<str>) {
    eprintln!("warning: {}", message.as_ref());
}

pub fn spinner(message: impl Into<String>) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("{spinner} {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_spinner()),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    pb.set_message(message.into());
    pb
}

pub fn confirm(message: &str, default: bool) -> Result<bool> {
    Ok(Confirm::new(message).with_default(default).prompt()?)
}

pub fn select(message: &str, options: Vec<String>) -> Result<String> {
    Ok(Select::new(message, options).prompt()?)
}

pub fn multiselect(message: &str, options: Vec<String>) -> Result<Vec<String>> {
    Ok(MultiSelect::new(message, options).prompt()?)
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

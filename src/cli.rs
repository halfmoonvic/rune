use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::config::{ConfigFormat, Theme};

#[derive(Debug, Parser)]
#[command(
    name = "rune",
    version,
    about = "CLI-driven egui dialogs and stream windows"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Form(FormArgs),
    Stream(StreamArgs),
    Confirm(ShortcutArgs),
    Alert(ShortcutArgs),
    Input(ShortcutArgs),
    Config(ConfigArgs),
}

#[derive(Clone, Debug, Args, Default)]
pub struct CommonWindowArgs {
    #[arg(long)]
    pub title: Option<String>,
    #[arg(long, value_name = "FILE")]
    pub style: Option<PathBuf>,
    #[arg(long)]
    pub width: Option<f32>,
    #[arg(long)]
    pub always_on_top: bool,
    #[arg(long, value_enum)]
    pub theme: Option<Theme>,
    #[arg(long)]
    pub timeout: Option<u64>,
}

#[derive(Clone, Debug, Args)]
pub struct FormArgs {
    #[command(flatten)]
    pub common: CommonWindowArgs,
    #[arg(long)]
    pub config: Option<PathBuf>,
    #[arg(long)]
    pub config_stdin: bool,
    #[arg(long, value_enum, default_value = "toml")]
    pub format: ConfigFormat,
    #[arg(long)]
    pub submit_label: Option<String>,
    #[arg(long)]
    pub cancel_label: Option<String>,
    #[arg(long = "text")]
    pub text_items: Vec<String>,
    #[arg(long = "markdown")]
    pub markdown_items: Vec<String>,
    #[arg(long = "input", value_parser = parse_id_label)]
    pub inputs: Vec<IdLabel>,
    #[arg(long = "textarea", value_parser = parse_id_label)]
    pub textareas: Vec<IdLabel>,
    #[arg(long = "select", value_parser = parse_id_label)]
    pub selects: Vec<IdLabel>,
    #[arg(long = "checkbox", value_parser = parse_id_label)]
    pub checkboxes: Vec<IdLabel>,
    #[arg(long = "default", value_parser = parse_id_value)]
    pub defaults: Vec<IdValue>,
    #[arg(long = "options", value_parser = parse_id_value)]
    pub options: Vec<IdValue>,
    #[arg(long = "required")]
    pub required: Vec<String>,
}

#[derive(Clone, Debug, Args)]
pub struct StreamArgs {
    #[command(flatten)]
    pub common: CommonWindowArgs,
    #[arg(long, default_value_t = 5000)]
    pub max_lines: usize,
    #[arg(long)]
    pub ansi: bool,
    #[arg(long, value_enum, default_value = "keep")]
    pub on_finish: OnFinish,
    #[arg(last = true)]
    pub command: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum OnFinish {
    Keep,
    Close,
}

#[derive(Clone, Debug, Args)]
pub struct ShortcutArgs {
    pub text: String,
    #[command(flatten)]
    pub common: CommonWindowArgs,
    #[arg(long)]
    pub default: Option<String>,
}

#[derive(Debug, Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommand,
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    Init,
    Show {
        #[arg(long, value_enum)]
        theme: Option<Theme>,
        #[arg(long)]
        always_on_top: bool,
    },
    Path,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdLabel {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdValue {
    pub id: String,
    pub value: String,
}

pub fn parse_id_label(value: &str) -> Result<IdLabel, String> {
    let (id, label) = value
        .split_once('=')
        .ok_or_else(|| "expected id=label".to_string())?;
    if id.trim().is_empty() {
        return Err("id must not be empty".to_string());
    }
    Ok(IdLabel {
        id: id.to_string(),
        label: label.to_string(),
    })
}

pub fn parse_id_value(value: &str) -> Result<IdValue, String> {
    let (id, parsed_value) = value
        .split_once('=')
        .ok_or_else(|| "expected id=value".to_string())?;
    if id.trim().is_empty() {
        return Err("id must not be empty".to_string());
    }
    Ok(IdValue {
        id: id.to_string(),
        value: parsed_value.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[test]
    fn parses_shorthand_id_label() {
        assert_eq!(
            parse_id_label("branch=Branch name").unwrap(),
            IdLabel {
                id: "branch".to_string(),
                label: "Branch name".to_string()
            }
        );
    }

    #[test]
    fn rejects_missing_shorthand_separator() {
        assert!(parse_id_label("branch").is_err());
    }

    #[test]
    fn parses_form_cli_flags() {
        let cli = Cli::parse_from([
            "rune",
            "form",
            "--title",
            "Deploy",
            "--style",
            "dark.toml",
            "--input",
            "branch=Branch",
            "--default",
            "branch=main",
            "--required",
            "branch",
        ]);

        let Command::Form(args) = cli.command else {
            panic!("expected form command");
        };
        assert_eq!(args.common.title.as_deref(), Some("Deploy"));
        assert_eq!(
            args.common.style.as_deref(),
            Some(std::path::Path::new("dark.toml"))
        );
        assert_eq!(args.inputs[0].id, "branch");
        assert_eq!(args.defaults[0].value, "main");
        assert_eq!(args.required[0], "branch");
    }

    #[test]
    fn parses_shortcut_style_flag() {
        let cli = Cli::parse_from(["rune", "input", "Name", "--style", "compact.toml"]);

        let Command::Input(args) = cli.command else {
            panic!("expected input command");
        };
        assert_eq!(
            args.common.style.as_deref(),
            Some(std::path::Path::new("compact.toml"))
        );
    }

    #[test]
    fn parses_stream_style_before_subprocess_command() {
        let cli = Cli::parse_from([
            "rune",
            "stream",
            "--style",
            "stream-style.toml",
            "--",
            "cargo",
            "test",
        ]);

        let Command::Stream(args) = cli.command else {
            panic!("expected stream command");
        };
        assert_eq!(
            args.common.style.as_deref(),
            Some(std::path::Path::new("stream-style.toml"))
        );
        assert_eq!(args.command, ["cargo", "test"]);
    }
}

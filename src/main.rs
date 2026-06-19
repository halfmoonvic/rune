mod cli;
mod config;
mod exit;
mod form;
mod stream;

use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;
use cli::{Cli, Command, ConfigCommand, ShortcutArgs};
use config::{
    StyleOverrides, global_config_path, init_global_config, load_global_style, load_style,
};
use form::{CallConfig, FormItem, FormOutcome, form_exit_code, resolve_form, run_form};
use stream::{StreamConfig, run_stream};

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code as u8),
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::from(exit::CONFIG_ERROR as u8)
        }
    }
}

fn run() -> Result<i32> {
    let cli = Cli::parse();
    match cli.command {
        Command::Config(args) => handle_config(args.command),
        Command::Form(args) => handle_form(args),
        Command::Confirm(args) => handle_confirm(args),
        Command::Alert(args) => handle_alert(args),
        Command::Input(args) => handle_input(args),
        Command::Stream(args) => handle_stream(args),
    }
}

fn handle_stream(args: cli::StreamArgs) -> Result<i32> {
    let style = load_style(args.common.style.as_deref())?;
    run_stream(StreamConfig::from(args), style)
}

fn handle_confirm(args: ShortcutArgs) -> Result<i32> {
    let style = load_style(args.common.style.as_deref())?;
    let config = shortcut_base_config(args, "Confirm", "Yes", "No", true);
    let outcome = run_form(config, style)?;
    Ok(match outcome {
        FormOutcome::Submitted(_) => exit::SUCCESS,
        FormOutcome::Cancelled => exit::CANCELLED,
        FormOutcome::TimedOut => exit::TIMEOUT,
    })
}

fn handle_alert(args: ShortcutArgs) -> Result<i32> {
    let style = load_style(args.common.style.as_deref())?;
    let config = shortcut_base_config(args, "Alert", "Close", "", false);
    let outcome = run_form(config, style)?;
    Ok(match outcome {
        FormOutcome::Submitted(_) | FormOutcome::Cancelled => exit::SUCCESS,
        FormOutcome::TimedOut => exit::TIMEOUT,
    })
}

fn handle_input(args: ShortcutArgs) -> Result<i32> {
    let style = load_style(args.common.style.as_deref())?;
    let text = args.text.clone();
    let default = args.default.clone().unwrap_or_default();
    let mut config = shortcut_base_config(args, "Input", "OK", "Cancel", true);
    config.items.clear();
    config.items.push(FormItem::Input {
        id: "value".to_string(),
        label: text,
        default,
        placeholder: String::new(),
        required: false,
        control_width: None,
    });

    let outcome = run_form(config, style)?;
    match outcome {
        FormOutcome::Submitted(output) => {
            let value: serde_json::Value =
                serde_json::from_str(&output).context("input form returned invalid JSON")?;
            if let Some(raw) = value.get("value").and_then(|value| value.as_str()) {
                println!("{raw}");
            }
            Ok(exit::SUCCESS)
        }
        FormOutcome::Cancelled => Ok(exit::CANCELLED),
        FormOutcome::TimedOut => Ok(exit::TIMEOUT),
    }
}

fn shortcut_base_config(
    args: ShortcutArgs,
    default_title: &str,
    submit_label: &str,
    cancel_label: &str,
    show_cancel: bool,
) -> CallConfig {
    let mut config = CallConfig {
        title: args
            .common
            .title
            .clone()
            .unwrap_or_else(|| default_title.to_string()),
        width: args.common.width,
        timeout: args.common.timeout,
        always_on_top: args.common.always_on_top,
        theme: args.common.theme,
        control_width: Default::default(),
        submit_label: submit_label.to_string(),
        cancel_label: cancel_label.to_string(),
        show_cancel,
        items: vec![FormItem::Text {
            content: args.text,
            style: form::TextStyle::Info,
        }],
    };

    if config.width.is_none() {
        config.width = Some(420.0);
    }
    config
}

fn handle_form(args: cli::FormArgs) -> Result<i32> {
    let style = load_style(args.common.style.as_deref())?;
    let config = resolve_form(args)?;
    let outcome = run_form(config, style)?;
    if let FormOutcome::Submitted(output) = &outcome {
        println!("{output}");
    }
    Ok(form_exit_code(&outcome))
}

fn handle_config(command: ConfigCommand) -> Result<i32> {
    match command {
        ConfigCommand::Init => {
            let path = init_global_config()?;
            println!("{}", path.display());
        }
        ConfigCommand::Show {
            theme,
            always_on_top,
        } => {
            let mut style = load_global_style();
            style.apply_overrides(StyleOverrides {
                theme,
                always_on_top,
            });
            print!("{}", config::show_style_config(&style)?);
        }
        ConfigCommand::Path => {
            println!("{}", global_config_path().display());
        }
    }
    Ok(exit::SUCCESS)
}

mod cli;
mod config;
mod exit;

use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Command, ConfigCommand};
use config::{StyleOverrides, global_config_path, init_global_config, load_global_style};

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
        Command::Form(_)
        | Command::Stream(_)
        | Command::Confirm(_)
        | Command::Alert(_)
        | Command::Input(_) => {
            eprintln!("error: command is not implemented yet");
            Ok(exit::CONFIG_ERROR)
        }
    }
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

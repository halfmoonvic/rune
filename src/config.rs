use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Default, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
#[value(rename_all = "kebab-case")]
pub enum Theme {
    Light,
    Dark,
    #[default]
    System,
    Custom,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct StyleConfig {
    pub window: WindowStyle,
}

impl Default for StyleConfig {
    fn default() -> Self {
        Self {
            window: WindowStyle::default(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct WindowStyle {
    pub theme: Theme,
    pub always_on_top: bool,
    pub corner_radius: f32,
    pub shadow: bool,
    pub header: HeaderStyle,
    pub body: BodyStyle,
    pub colors: Option<ColorStyle>,
}

impl Default for WindowStyle {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            always_on_top: false,
            corner_radius: 8.0,
            shadow: true,
            header: HeaderStyle::default(),
            body: BodyStyle::default(),
            colors: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct HeaderStyle {
    pub height: f32,
    pub font_size: f32,
    pub show_icon: bool,
}

impl Default for HeaderStyle {
    fn default() -> Self {
        Self {
            height: 40.0,
            font_size: 16.0,
            show_icon: true,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct BodyStyle {
    pub padding: f32,
    pub font_size: f32,
    pub line_height: f32,
}

impl Default for BodyStyle {
    fn default() -> Self {
        Self {
            padding: 16.0,
            font_size: 14.0,
            line_height: 1.5,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ColorStyle {
    pub background: String,
    pub text: String,
    pub accent: String,
    pub border: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum ConfigFormat {
    Toml,
    Json,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StyleOverrides {
    pub theme: Option<Theme>,
    pub always_on_top: bool,
}

impl StyleConfig {
    pub fn apply_overrides(&mut self, overrides: StyleOverrides) {
        if let Some(theme) = overrides.theme {
            self.window.theme = theme;
        }
        if overrides.always_on_top {
            self.window.always_on_top = true;
        }
    }
}

pub fn global_config_path() -> PathBuf {
    if let Some(project_dirs) = ProjectDirs::from("", "", "rune") {
        project_dirs.config_dir().join("config.toml")
    } else {
        PathBuf::from("config.toml")
    }
}

pub fn load_global_style() -> StyleConfig {
    let path = global_config_path();
    match fs::read_to_string(&path) {
        Ok(text) => match toml::from_str::<StyleConfig>(&text) {
            Ok(style) => style,
            Err(err) => {
                eprintln!(
                    "warning: failed to parse global style config {}: {err}",
                    path.display()
                );
                StyleConfig::default()
            }
        },
        Err(err) if err.kind() == io::ErrorKind::NotFound => StyleConfig::default(),
        Err(err) => {
            eprintln!(
                "warning: failed to read global style config {}: {err}",
                path.display()
            );
            StyleConfig::default()
        }
    }
}

pub fn parse_config_text<T>(text: &str, format: ConfigFormat) -> Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    match format {
        ConfigFormat::Toml => toml::from_str(text).context("failed to parse TOML config"),
        ConfigFormat::Json => serde_json::from_str(text).context("failed to parse JSON config"),
    }
}

pub fn parse_config_file<T>(path: &Path) -> Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read config file {}", path.display()))?;
    parse_config_text(&text, format_from_path(path))
}

pub fn parse_config_stdin<T>(format: ConfigFormat) -> Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    let mut text = String::new();
    io::stdin()
        .read_to_string(&mut text)
        .context("failed to read config from stdin")?;
    parse_config_text(&text, format)
}

pub fn format_from_path(path: &Path) -> ConfigFormat {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("json") => ConfigFormat::Json,
        _ => ConfigFormat::Toml,
    }
}

pub fn init_global_config() -> Result<PathBuf> {
    let path = global_config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create config directory {}", parent.display()))?;
    } else {
        bail!("global config path has no parent directory");
    }

    if !path.exists() {
        fs::write(&path, DEFAULT_GLOBAL_CONFIG)
            .with_context(|| format!("failed to write config file {}", path.display()))?;
    }

    Ok(path)
}

pub fn show_style_config(style: &StyleConfig) -> Result<String> {
    toml::to_string_pretty(style).context("failed to serialize style config")
}

pub const DEFAULT_GLOBAL_CONFIG: &str = r##"# Rune global style config

[window]
theme = "system"
always_on_top = false
corner_radius = 8.0
shadow = true

[window.header]
height = 40.0
font_size = 16.0
show_icon = true

[window.body]
padding = 16.0
font_size = 14.0
line_height = 1.5

# [window.colors]
# background = "#1e1e1e"
# text = "#e0e0e0"
# accent = "#5b8def"
# border = "#333333"
"##;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_partial_global_style() {
        let style: StyleConfig = parse_config_text(
            r#"
            [window]
            theme = "dark"
            [window.body]
            padding = 20
            "#,
            ConfigFormat::Toml,
        )
        .unwrap();

        assert_eq!(style.window.theme, Theme::Dark);
        assert_eq!(style.window.body.padding, 20.0);
        assert_eq!(style.window.header.height, 40.0);
    }

    #[test]
    fn cli_style_overrides_take_precedence() {
        let mut style = StyleConfig::default();
        style.window.theme = Theme::Light;
        style.apply_overrides(StyleOverrides {
            theme: Some(Theme::Dark),
            always_on_top: true,
        });

        assert_eq!(style.window.theme, Theme::Dark);
        assert!(style.window.always_on_top);
    }
}

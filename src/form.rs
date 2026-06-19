use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    path::{Path, PathBuf},
    sync::Arc as StdArc,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail};
use eframe::egui;
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    cli::{FormArgs, IdLabel, IdValue},
    config::{
        ConfigFormat, StyleConfig, StyleOverrides, Theme, parse_config_file, parse_config_stdin,
    },
    exit,
};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct CallConfig {
    pub title: String,
    pub width: Option<f32>,
    pub timeout: Option<u64>,
    pub always_on_top: bool,
    pub theme: Option<Theme>,
    pub submit_label: String,
    pub cancel_label: String,
    pub items: Vec<FormItem>,
    #[serde(skip, default = "default_show_cancel")]
    pub show_cancel: bool,
}

impl Default for CallConfig {
    fn default() -> Self {
        Self {
            title: "Rune".to_string(),
            width: None,
            timeout: None,
            always_on_top: false,
            theme: None,
            submit_label: "OK".to_string(),
            cancel_label: "Cancel".to_string(),
            items: Vec::new(),
            show_cancel: true,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum FormItem {
    Text {
        content: String,
        #[serde(default)]
        style: TextStyle,
    },
    Markdown {
        content: String,
    },
    Input {
        id: String,
        label: String,
        #[serde(default)]
        default: String,
        #[serde(default)]
        placeholder: String,
        #[serde(default)]
        required: bool,
    },
    Textarea {
        id: String,
        label: String,
        #[serde(default)]
        default: String,
        #[serde(default = "default_rows")]
        rows: usize,
        #[serde(default)]
        required: bool,
    },
    Select {
        id: String,
        label: String,
        options: Vec<String>,
        #[serde(default)]
        default: Option<String>,
        #[serde(default)]
        required: bool,
    },
    Checkbox {
        id: String,
        label: String,
        #[serde(default)]
        default: bool,
        #[serde(default)]
        required: bool,
    },
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TextStyle {
    #[default]
    Info,
    Warning,
    Danger,
}

fn default_rows() -> usize {
    4
}

fn default_show_cancel() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq)]
enum FieldValue {
    Text(String),
    Bool(bool),
}

#[derive(Clone, Debug, PartialEq)]
pub enum FormOutcome {
    Submitted(String),
    Cancelled,
    TimedOut,
}

pub fn resolve_form(args: FormArgs) -> Result<CallConfig> {
    if args.config.is_some() && args.config_stdin {
        bail!("--config and --config-stdin cannot be used together");
    }

    let mut config = if let Some(path) = args.config.as_ref() {
        parse_config_file::<CallConfig>(path)?
    } else if args.config_stdin {
        parse_config_stdin::<CallConfig>(args.format)?
    } else {
        CallConfig::default()
    };

    apply_form_args(&mut config, &args)?;
    validate_config(&config)?;
    Ok(config)
}

pub fn run_form(config: CallConfig, mut style: StyleConfig) -> Result<FormOutcome> {
    style.apply_overrides(StyleOverrides {
        theme: config.theme,
        always_on_top: config.always_on_top,
    });

    let result = Arc::new(Mutex::new(None));
    let app_result = Arc::clone(&result);
    let width = config.width.unwrap_or(480.0);
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(config.title.clone())
        .with_inner_size([width, 360.0]);
    if style.window.always_on_top {
        viewport = viewport.with_always_on_top();
    }

    let native_options = native_options(viewport);
    let title = config.title.clone();
    eframe::run_native(
        &title,
        native_options,
        Box::new(move |cc| {
            install_system_cjk_font(&cc.egui_ctx);
            apply_egui_style(&cc.egui_ctx, &style);
            Ok(Box::new(FormApp::new(config, app_result)))
        }),
    )
    .map_err(|err| anyhow!("failed to run form window: {err}"))?;

    Ok(result
        .lock()
        .expect("form result lock poisoned")
        .clone()
        .unwrap_or(FormOutcome::Cancelled))
}

fn apply_form_args(config: &mut CallConfig, args: &FormArgs) -> Result<()> {
    if let Some(title) = args.common.title.as_ref() {
        config.title = title.clone();
    }
    if let Some(width) = args.common.width {
        config.width = Some(width);
    }
    if let Some(timeout) = args.common.timeout {
        config.timeout = Some(timeout);
    }
    if let Some(theme) = args.common.theme {
        config.theme = Some(theme);
    }
    if args.common.always_on_top {
        config.always_on_top = true;
    }
    if let Some(submit_label) = args.submit_label.as_ref() {
        config.submit_label = submit_label.clone();
    }
    if let Some(cancel_label) = args.cancel_label.as_ref() {
        config.cancel_label = cancel_label.clone();
    }

    let mut defaults = id_value_map(&args.defaults, "--default")?;
    let mut options = id_value_map(&args.options, "--options")?;
    let required: BTreeSet<&str> = args.required.iter().map(String::as_str).collect();

    for text in &args.text_items {
        config.items.push(FormItem::Text {
            content: text.clone(),
            style: TextStyle::Info,
        });
    }
    for text in &args.markdown_items {
        config.items.push(FormItem::Markdown {
            content: text.clone(),
        });
    }
    for item in &args.inputs {
        config.items.push(FormItem::Input {
            id: item.id.clone(),
            label: item.label.clone(),
            default: defaults.remove(&item.id).unwrap_or_default(),
            placeholder: String::new(),
            required: required.contains(item.id.as_str()),
        });
    }
    for item in &args.textareas {
        config.items.push(FormItem::Textarea {
            id: item.id.clone(),
            label: item.label.clone(),
            default: defaults.remove(&item.id).unwrap_or_default(),
            rows: default_rows(),
            required: required.contains(item.id.as_str()),
        });
    }
    for item in &args.selects {
        let parsed_options = options
            .remove(&item.id)
            .map(|raw| split_options(&raw))
            .unwrap_or_default();
        config.items.push(FormItem::Select {
            id: item.id.clone(),
            label: item.label.clone(),
            default: defaults.remove(&item.id),
            options: parsed_options,
            required: required.contains(item.id.as_str()),
        });
    }
    for item in &args.checkboxes {
        let default = defaults
            .remove(&item.id)
            .map(|value| value.parse::<bool>())
            .transpose()
            .with_context(|| format!("invalid boolean default for '{}'", item.id))?
            .unwrap_or(false);
        config.items.push(FormItem::Checkbox {
            id: item.id.clone(),
            label: item.label.clone(),
            default,
            required: required.contains(item.id.as_str()),
        });
    }

    if !defaults.is_empty() {
        bail!(
            "--default provided for unknown item id '{}'",
            first_key(&defaults)
        );
    }
    if !options.is_empty() {
        bail!(
            "--options provided for unknown select id '{}'",
            first_key(&options)
        );
    }

    Ok(())
}

fn id_value_map(values: &[IdValue], flag: &str) -> Result<HashMap<String, String>> {
    let mut map = HashMap::new();
    for value in values {
        if map.insert(value.id.clone(), value.value.clone()).is_some() {
            bail!("{flag} was provided more than once for '{}'", value.id);
        }
    }
    Ok(map)
}

fn first_key(map: &HashMap<String, String>) -> &str {
    map.keys().next().map(String::as_str).unwrap_or("")
}

fn split_options(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

pub fn validate_config(config: &CallConfig) -> Result<()> {
    let mut ids = BTreeSet::new();
    for item in &config.items {
        match item {
            FormItem::Text { .. } | FormItem::Markdown { .. } => {}
            FormItem::Input { id, .. }
            | FormItem::Textarea { id, .. }
            | FormItem::Select { id, .. }
            | FormItem::Checkbox { id, .. } => {
                if id.trim().is_empty() {
                    bail!("interactive item id must not be empty");
                }
                if !ids.insert(id.clone()) {
                    bail!("duplicate interactive item id '{id}'");
                }
            }
        }

        if let FormItem::Select {
            id,
            options,
            default,
            ..
        } = item
        {
            if options.is_empty() {
                bail!("select item '{id}' must define at least one option");
            }
            if let Some(default) = default
                && !options.contains(default)
            {
                bail!("select item '{id}' default must be one of its options");
            }
        }
    }
    Ok(())
}

struct FormApp {
    config: CallConfig,
    values: BTreeMap<String, FieldValue>,
    errors: BTreeMap<String, String>,
    markdown_cache: CommonMarkCache,
    result: Arc<Mutex<Option<FormOutcome>>>,
    started_at: Instant,
}

impl FormApp {
    fn new(config: CallConfig, result: Arc<Mutex<Option<FormOutcome>>>) -> Self {
        let mut values = BTreeMap::new();
        for item in &config.items {
            match item {
                FormItem::Input { id, default, .. } | FormItem::Textarea { id, default, .. } => {
                    values.insert(id.clone(), FieldValue::Text(default.clone()));
                }
                FormItem::Select {
                    id,
                    options,
                    default,
                    ..
                } => {
                    values.insert(
                        id.clone(),
                        FieldValue::Text(default.clone().unwrap_or_else(|| options[0].clone())),
                    );
                }
                FormItem::Checkbox { id, default, .. } => {
                    values.insert(id.clone(), FieldValue::Bool(*default));
                }
                FormItem::Text { .. } | FormItem::Markdown { .. } => {}
            }
        }

        Self {
            config,
            values,
            errors: BTreeMap::new(),
            markdown_cache: CommonMarkCache::default(),
            result,
            started_at: Instant::now(),
        }
    }

    fn submit(&mut self, ctx: &egui::Context) {
        self.errors = self.validate_values();
        if self.errors.is_empty() {
            let json = self.output_json();
            *self.result.lock().expect("form result lock poisoned") =
                Some(FormOutcome::Submitted(json));
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn cancel(&self, ctx: &egui::Context) {
        *self.result.lock().expect("form result lock poisoned") = Some(FormOutcome::Cancelled);
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    fn validate_values(&self) -> BTreeMap<String, String> {
        let mut errors = BTreeMap::new();
        for item in &self.config.items {
            match item {
                FormItem::Input {
                    id, required: true, ..
                }
                | FormItem::Textarea {
                    id, required: true, ..
                } => {
                    if let Some(FieldValue::Text(value)) = self.values.get(id)
                        && value.trim().is_empty()
                    {
                        errors.insert(id.clone(), "Required".to_string());
                    }
                }
                FormItem::Checkbox {
                    id, required: true, ..
                } => {
                    if !matches!(self.values.get(id), Some(FieldValue::Bool(true))) {
                        errors.insert(id.clone(), "Required".to_string());
                    }
                }
                _ => {}
            }
        }
        errors
    }

    fn output_json(&self) -> String {
        let mut output = serde_json::Map::new();
        for (id, value) in &self.values {
            let json_value = match value {
                FieldValue::Text(value) => Value::String(value.clone()),
                FieldValue::Bool(value) => Value::Bool(*value),
            };
            output.insert(id.clone(), json_value);
        }
        Value::Object(output).to_string()
    }
}

impl eframe::App for FormApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if let Some(timeout) = self.config.timeout
            && self.started_at.elapsed() >= Duration::from_secs(timeout)
        {
            *self.result.lock().expect("form result lock poisoned") = Some(FormOutcome::TimedOut);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        egui::Frame::central_panel(ui.style()).show(ui, |ui| {
            ui.vertical_centered_justified(|ui| {
                ui.heading(&self.config.title);
            });
            ui.add_space(10.0);

            egui::ScrollArea::vertical()
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    let items = self.config.items.clone();
                    for item in items {
                        self.render_item(ui, &item);
                        ui.add_space(8.0);
                    }
                });

            ui.separator();
            ui.horizontal(|ui| {
                if self.config.show_cancel && ui.button(&self.config.cancel_label).clicked() {
                    self.cancel(&ctx);
                }
                if ui.button(&self.config.submit_label).clicked() {
                    self.submit(&ctx);
                }
            });
        });
    }
}

impl FormApp {
    fn render_item(&mut self, ui: &mut egui::Ui, item: &FormItem) {
        match item {
            FormItem::Text { content, style } => {
                let color = match style {
                    TextStyle::Info => ui.visuals().text_color(),
                    TextStyle::Warning => egui::Color32::from_rgb(180, 120, 0),
                    TextStyle::Danger => egui::Color32::from_rgb(190, 50, 50),
                };
                ui.label(egui::RichText::new(content).color(color));
            }
            FormItem::Markdown { content } => {
                CommonMarkViewer::new().show(ui, &mut self.markdown_cache, content);
            }
            FormItem::Input {
                id,
                label,
                placeholder,
                ..
            } => {
                ui.label(label);
                if let Some(FieldValue::Text(value)) = self.values.get_mut(id) {
                    let edit = egui::TextEdit::singleline(value).hint_text(placeholder);
                    ui.add(edit);
                }
                self.render_error(ui, id);
            }
            FormItem::Textarea {
                id, label, rows, ..
            } => {
                ui.label(label);
                if let Some(FieldValue::Text(value)) = self.values.get_mut(id) {
                    ui.add(egui::TextEdit::multiline(value).desired_rows(*rows));
                }
                self.render_error(ui, id);
            }
            FormItem::Select {
                id, label, options, ..
            } => {
                ui.label(label);
                if let Some(FieldValue::Text(value)) = self.values.get_mut(id) {
                    egui::ComboBox::from_id_salt(id)
                        .selected_text(value.as_str())
                        .show_ui(ui, |ui| {
                            for option in options {
                                ui.selectable_value(value, option.clone(), option);
                            }
                        });
                }
                self.render_error(ui, id);
            }
            FormItem::Checkbox { id, label, .. } => {
                if let Some(FieldValue::Bool(value)) = self.values.get_mut(id) {
                    ui.checkbox(value, label);
                }
                self.render_error(ui, id);
            }
        }
    }

    fn render_error(&self, ui: &mut egui::Ui, id: &str) {
        if let Some(error) = self.errors.get(id) {
            ui.label(egui::RichText::new(error).color(egui::Color32::from_rgb(190, 50, 50)));
        }
    }
}

pub(crate) fn apply_egui_style(ctx: &egui::Context, style: &StyleConfig) {
    match style.window.theme {
        Theme::Light => ctx.set_visuals(egui::Visuals::light()),
        Theme::Dark | Theme::Custom => ctx.set_visuals(egui::Visuals::dark()),
        Theme::System => {}
    }

    let mut egui_style = (*ctx.global_style()).clone();
    egui_style.spacing.item_spacing = egui::vec2(8.0, style.window.body.padding / 2.0);
    egui_style.spacing.window_margin = egui::Margin::same(style.window.body.padding as i8);
    egui_style.text_styles.insert(
        egui::TextStyle::Body,
        egui::FontId::proportional(style.window.body.font_size),
    );
    egui_style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::proportional(style.window.header.font_size + 6.0),
    );
    ctx.set_global_style(egui_style);
}

pub(crate) fn native_options(viewport: egui::ViewportBuilder) -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport,
        #[cfg(target_os = "macos")]
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    }
}

pub(crate) fn install_system_cjk_font(ctx: &egui::Context) {
    let Some((name, bytes)) = load_system_cjk_font() else {
        return;
    };

    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert(name.clone(), StdArc::new(egui::FontData::from_owned(bytes)));

    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        if let Some(fonts_for_family) = fonts.families.get_mut(&family) {
            fonts_for_family.push(name.clone());
        }
    }

    ctx.set_fonts(fonts);
}

fn load_system_cjk_font() -> Option<(String, Vec<u8>)> {
    system_cjk_font_paths().into_iter().find_map(|path| {
        fs::read(&path)
            .ok()
            .map(|bytes| (path.to_string_lossy().into_owned(), bytes))
    })
}

#[cfg(target_os = "macos")]
fn system_cjk_font_paths() -> Vec<PathBuf> {
    vec![
        Path::new("/System/Library/Fonts/STHeiti Medium.ttc").to_path_buf(),
        Path::new("/System/Library/Fonts/Hiragino Sans GB.ttc").to_path_buf(),
        Path::new("/Library/Fonts/Arial Unicode.ttf").to_path_buf(),
        Path::new("/System/Library/Fonts/Supplemental/Arial Unicode.ttf").to_path_buf(),
    ]
}

#[cfg(target_os = "windows")]
fn system_cjk_font_paths() -> Vec<PathBuf> {
    let windir = std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    let fonts = windir.join("Fonts");
    vec![
        fonts.join("msyh.ttc"),
        fonts.join("simhei.ttf"),
        fonts.join("simsun.ttc"),
    ]
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
fn system_cjk_font_paths() -> Vec<PathBuf> {
    vec![
        Path::new("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc").to_path_buf(),
        Path::new("/usr/share/fonts/opentype/noto/NotoSansCJKsc-Regular.otf").to_path_buf(),
        Path::new("/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc").to_path_buf(),
        Path::new("/usr/share/fonts/truetype/wqy/wqy-microhei.ttc").to_path_buf(),
        Path::new("/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf").to_path_buf(),
    ]
}

pub fn form_exit_code(outcome: &FormOutcome) -> i32 {
    match outcome {
        FormOutcome::Submitted(_) => exit::SUCCESS,
        FormOutcome::Cancelled => exit::CANCELLED,
        FormOutcome::TimedOut => exit::TIMEOUT,
    }
}

#[allow(dead_code)]
fn _keep_cli_types_used(_: ConfigFormat, _: IdLabel) {}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;
    use crate::cli::{Cli, Command};

    #[test]
    fn validates_duplicate_interactive_ids() {
        let config = CallConfig {
            items: vec![
                FormItem::Input {
                    id: "name".to_string(),
                    label: "Name".to_string(),
                    default: String::new(),
                    placeholder: String::new(),
                    required: false,
                },
                FormItem::Checkbox {
                    id: "name".to_string(),
                    label: "Name".to_string(),
                    default: false,
                    required: false,
                },
            ],
            ..Default::default()
        };
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn validates_select_default() {
        let config = CallConfig {
            items: vec![FormItem::Select {
                id: "env".to_string(),
                label: "Environment".to_string(),
                options: vec!["dev".to_string(), "prod".to_string()],
                default: Some("staging".to_string()),
                required: false,
            }],
            ..Default::default()
        };
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn parses_json_call_config() {
        let config: CallConfig = crate::config::parse_config_text(
            r#"{
                "title": "Deploy",
                "items": [
                    {"type":"input","id":"branch","label":"Branch","default":"main","required":true}
                ]
            }"#,
            ConfigFormat::Json,
        )
        .unwrap();

        assert_eq!(config.title, "Deploy");
        assert_eq!(config.items.len(), 1);
    }

    #[test]
    fn parses_toml_call_config() {
        let config: CallConfig = crate::config::parse_config_text(
            r#"
            title = "Deploy"

            [[items]]
            type = "checkbox"
            id = "confirm"
            label = "Confirm"
            required = true
            "#,
            ConfigFormat::Toml,
        )
        .unwrap();

        assert_eq!(config.title, "Deploy");
        assert!(matches!(
            config.items.first(),
            Some(FormItem::Checkbox { required: true, .. })
        ));
        assert!(config.show_cancel);
    }

    #[test]
    fn cli_flags_override_call_config() {
        let path = std::env::temp_dir().join(format!(
            "rune-form-{}.toml",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(
            &path,
            r#"
            title = "From config"
            width = 320

            [[items]]
            type = "select"
            id = "env"
            label = "Environment"
            options = ["dev", "prod"]
            default = "dev"
            "#,
        )
        .unwrap();

        let cli = Cli::parse_from([
            "rune",
            "form",
            "--config",
            path.to_str().unwrap(),
            "--title",
            "From CLI",
            "--width",
            "640",
        ]);
        let Command::Form(args) = cli.command else {
            panic!("expected form command");
        };
        let config = resolve_form(args).unwrap();
        std::fs::remove_file(path).unwrap();

        assert_eq!(config.title, "From CLI");
        assert_eq!(config.width, Some(640.0));
    }

    #[test]
    fn merges_cli_shorthand_into_items() {
        let cli = Cli::parse_from([
            "rune",
            "form",
            "--select",
            "env=Environment",
            "--options",
            "env=dev,staging,prod",
            "--default",
            "env=staging",
        ]);
        let Command::Form(args) = cli.command else {
            panic!("expected form command");
        };
        let config = resolve_form(args).unwrap();

        assert_eq!(
            config.items,
            vec![FormItem::Select {
                id: "env".to_string(),
                label: "Environment".to_string(),
                options: vec!["dev".to_string(), "staging".to_string(), "prod".to_string()],
                default: Some("staging".to_string()),
                required: false,
            }]
        );
    }

    #[test]
    fn output_json_is_single_line() {
        let app = FormApp::new(
            CallConfig {
                items: vec![FormItem::Checkbox {
                    id: "ok".to_string(),
                    label: "OK".to_string(),
                    default: true,
                    required: false,
                }],
                ..Default::default()
            },
            Arc::new(Mutex::new(None)),
        );

        assert_eq!(app.output_json(), r#"{"ok":true}"#);
    }
}

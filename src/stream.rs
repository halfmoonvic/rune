use std::{
    collections::VecDeque,
    io::{self, BufRead, BufReader},
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{Result, anyhow, bail};
use eframe::egui;

use crate::{
    cli::{OnFinish, StreamArgs},
    config::{StyleConfig, StyleOverrides},
    exit,
    form::apply_egui_style,
};

#[derive(Clone, Debug)]
pub struct StreamConfig {
    pub title: String,
    pub width: Option<f32>,
    pub timeout: Option<u64>,
    pub always_on_top: bool,
    pub theme: Option<crate::config::Theme>,
    pub max_lines: usize,
    pub ansi: bool,
    pub on_finish: OnFinish,
    pub command: Vec<String>,
}

impl From<StreamArgs> for StreamConfig {
    fn from(args: StreamArgs) -> Self {
        Self {
            title: args.common.title.unwrap_or_else(|| "Rune".to_string()),
            width: args.common.width,
            timeout: args.common.timeout,
            always_on_top: args.common.always_on_top,
            theme: args.common.theme,
            max_lines: args.max_lines,
            ansi: args.ansi,
            on_finish: args.on_finish,
            command: args.command,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StreamEvent {
    Line(String),
    Finished(i32),
    Stopped,
    Error(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum StreamStatus {
    Running,
    Finished(i32),
    Stopped,
    Error(String),
}

impl StreamStatus {
    fn label(&self) -> String {
        match self {
            StreamStatus::Running => "Running...".to_string(),
            StreamStatus::Finished(code) => format!("Exited (code {code})"),
            StreamStatus::Stopped => "Stopped".to_string(),
            StreamStatus::Error(error) => format!("Error: {error}"),
        }
    }

    fn is_running(&self) -> bool {
        matches!(self, StreamStatus::Running)
    }
}

pub fn run_stream(config: StreamConfig, mut style: StyleConfig) -> Result<i32> {
    if config.max_lines == 0 {
        bail!("--max-lines must be greater than zero");
    }

    style.apply_overrides(StyleOverrides {
        theme: config.theme,
        always_on_top: config.always_on_top,
    });

    let (event_tx, event_rx) = mpsc::channel();
    let stop_tx = if config.command.is_empty() {
        spawn_stdin_reader(event_tx);
        None
    } else {
        Some(spawn_child_reader(config.command.clone(), event_tx)?)
    };

    let exit_code = Arc::new(Mutex::new(exit::SUCCESS));
    let app_exit_code = Arc::clone(&exit_code);
    let width = config.width.unwrap_or(720.0);
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(config.title.clone())
        .with_inner_size([width, 460.0]);
    if style.window.always_on_top {
        viewport = viewport.with_always_on_top();
    }
    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    let title = config.title.clone();

    eframe::run_native(
        &title,
        native_options,
        Box::new(move |cc| {
            apply_egui_style(&cc.egui_ctx, &style);
            Ok(Box::new(StreamApp::new(
                config,
                event_rx,
                stop_tx,
                app_exit_code,
            )))
        }),
    )
    .map_err(|err| anyhow!("failed to run stream window: {err}"))?;

    Ok(*exit_code.lock().expect("stream exit code lock poisoned"))
}

fn spawn_stdin_reader(event_tx: Sender<StreamEvent>) {
    thread::spawn(move || {
        let stdin = io::stdin();
        let reader = stdin.lock();
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    if event_tx.send(StreamEvent::Line(line)).is_err() {
                        return;
                    }
                }
                Err(err) => {
                    _ = event_tx.send(StreamEvent::Error(err.to_string()));
                    return;
                }
            }
        }
        _ = event_tx.send(StreamEvent::Finished(exit::SUCCESS));
    });
}

fn spawn_child_reader(command: Vec<String>, event_tx: Sender<StreamEvent>) -> Result<Sender<()>> {
    let program = command
        .first()
        .ok_or_else(|| anyhow!("subprocess command cannot be empty"))?;
    let mut child = Command::new(program)
        .args(&command[1..])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| anyhow!("failed to spawn subprocess '{}': {err}", command.join(" ")))?;

    if let Some(stdout) = child.stdout.take() {
        spawn_reader(stdout, event_tx.clone());
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_reader(stderr, event_tx.clone());
    }

    let (stop_tx, stop_rx) = mpsc::channel();
    thread::spawn(move || {
        loop {
            if stop_rx.try_recv().is_ok() {
                terminate_child(&mut child);
                _ = event_tx.send(StreamEvent::Stopped);
                return;
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    _ = event_tx.send(StreamEvent::Finished(status.code().unwrap_or(1)));
                    return;
                }
                Ok(None) => thread::sleep(Duration::from_millis(50)),
                Err(err) => {
                    _ = event_tx.send(StreamEvent::Error(err.to_string()));
                    return;
                }
            }
        }
    });

    Ok(stop_tx)
}

fn spawn_reader<R>(reader: R, event_tx: Sender<StreamEvent>)
where
    R: io::Read + Send + 'static,
{
    thread::spawn(move || {
        for line in BufReader::new(reader).lines() {
            match line {
                Ok(line) => {
                    if event_tx.send(StreamEvent::Line(line)).is_err() {
                        return;
                    }
                }
                Err(err) => {
                    _ = event_tx.send(StreamEvent::Error(err.to_string()));
                    return;
                }
            }
        }
    });
}

#[cfg(unix)]
fn terminate_child(child: &mut std::process::Child) {
    let pid = child.id().to_string();
    _ = Command::new("kill").args(["-TERM", &pid]).status();
    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(700) {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(_) => break,
        }
    }
    _ = child.kill();
    _ = child.wait();
}

#[cfg(not(unix))]
fn terminate_child(child: &mut std::process::Child) {
    _ = child.kill();
    _ = child.wait();
}

pub struct LineBuffer {
    max_lines: usize,
    lines: VecDeque<String>,
}

impl LineBuffer {
    pub fn new(max_lines: usize) -> Self {
        Self {
            max_lines,
            lines: VecDeque::with_capacity(max_lines),
        }
    }

    pub fn push(&mut self, line: String) {
        if self.lines.len() == self.max_lines {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
    }

    fn iter(&self) -> impl Iterator<Item = &String> {
        self.lines.iter()
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.lines.len()
    }
}

struct StreamApp {
    config: StreamConfig,
    lines: LineBuffer,
    events: Receiver<StreamEvent>,
    stop_tx: Option<Sender<()>>,
    status: StreamStatus,
    exit_code: Arc<Mutex<i32>>,
    stopped_by_user: bool,
    started_at: Instant,
}

impl StreamApp {
    fn new(
        config: StreamConfig,
        events: Receiver<StreamEvent>,
        stop_tx: Option<Sender<()>>,
        exit_code: Arc<Mutex<i32>>,
    ) -> Self {
        Self {
            lines: LineBuffer::new(config.max_lines),
            config,
            events,
            stop_tx,
            status: StreamStatus::Running,
            exit_code,
            stopped_by_user: false,
            started_at: Instant::now(),
        }
    }

    fn drain_events(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.events.try_recv() {
            match event {
                StreamEvent::Line(line) => self.lines.push(if self.config.ansi {
                    strip_ansi(&line)
                } else {
                    line
                }),
                StreamEvent::Finished(code) => {
                    self.status = StreamStatus::Finished(code);
                    *self
                        .exit_code
                        .lock()
                        .expect("stream exit code lock poisoned") = code;
                    if self.config.on_finish == OnFinish::Close {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
                StreamEvent::Stopped => {
                    self.status = StreamStatus::Stopped;
                    *self
                        .exit_code
                        .lock()
                        .expect("stream exit code lock poisoned") = exit::STOPPED;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                StreamEvent::Error(error) => {
                    self.status = StreamStatus::Error(error);
                    *self
                        .exit_code
                        .lock()
                        .expect("stream exit code lock poisoned") = exit::CONFIG_ERROR;
                }
            }
            ctx.request_repaint();
        }
    }

    fn stop(&mut self) {
        self.stopped_by_user = true;
        if let Some(stop_tx) = self.stop_tx.take() {
            _ = stop_tx.send(());
        }
    }
}

impl eframe::App for StreamApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.drain_events(&ctx);

        if let Some(timeout) = self.config.timeout
            && self.status.is_running()
            && self.started_at.elapsed() >= Duration::from_secs(timeout)
        {
            *self
                .exit_code
                .lock()
                .expect("stream exit code lock poisoned") = exit::TIMEOUT;
            self.stop();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        egui::Frame::central_panel(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(&self.config.title);
                ui.separator();
                ui.label(self.status.label());
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for line in self.lines.iter() {
                        ui.label(egui::RichText::new(line).monospace());
                    }
                });
            ui.separator();
            ui.horizontal(|ui| {
                if self.stop_tx.is_some() && self.status.is_running() {
                    if ui.button("Stop").clicked() {
                        self.stop();
                    }
                } else if ui.button("Close").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        });
    }

    fn on_exit(&mut self) {
        if self.status.is_running() && self.stop_tx.is_some() && !self.stopped_by_user {
            self.stop();
        }
    }
}

fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for seq in chars.by_ref() {
                if seq.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_buffer_caps_old_lines() {
        let mut buffer = LineBuffer::new(2);
        buffer.push("one".to_string());
        buffer.push("two".to_string());
        buffer.push("three".to_string());

        assert_eq!(buffer.len(), 2);
        assert_eq!(
            buffer.iter().cloned().collect::<Vec<_>>(),
            vec!["two".to_string(), "three".to_string()]
        );
    }

    #[test]
    fn strips_basic_ansi_sequences() {
        assert_eq!(strip_ansi("\u{1b}[31mred\u{1b}[0m"), "red");
    }
}

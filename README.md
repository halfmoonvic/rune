# Rune

Rune is a small cross-platform desktop helper for shell scripts. It provides
CLI-driven dialogs and stream windows built with Rust, `eframe`, and `egui`.

It is designed for cases where a script needs a lightweight GUI surface:
confirmation prompts, alerts, single-value input, structured forms, or live log
output.

## Features

- `form`: render a structured form from CLI flags, TOML, JSON, or stdin.
- `stream`: show stdin or a subprocess output in a live desktop window.
- `confirm`, `alert`, `input`: shortcut commands for common one-step dialogs.
- Global style config with per-call overrides.
- JSON output for submitted forms, suitable for scripts.
- Cross-platform Rust implementation.

## Install

```bash
cargo install --path .
```

For local development, run commands through Cargo:

```bash
cargo run -- form --text "Hello from Rune"
```

## Quick Start

Show an alert:

```bash
rune alert "Build finished"
```

Ask for confirmation:

```bash
rune confirm "Deploy to production?"
```

Read one input value:

```bash
rune input "Branch name" --default main
```

Run a form from a config file:

```bash
rune form --config examples/demo-form.toml
```

Run the search demo:

```bash
rune form --config examples/search-demo.toml
```

Show a command's output in a stream window:

```bash
rune stream -- cargo test
```

Pipe logs into a stream window:

```bash
cargo test 2>&1 | rune stream --title "Tests"
```

## Commands

```text
rune form [OPTIONS]
rune stream [OPTIONS] [-- CMD...]
rune confirm <TEXT> [OPTIONS]
rune alert <TEXT> [OPTIONS]
rune input <TEXT> [OPTIONS]
rune config <init|show|path>
```

Common window options:

```text
--title <TITLE>
--style <FILE>
--width <PX>
--always-on-top
--theme <light|dark|system|custom>
--timeout <SECS>
```

## Form Config

`form --config` describes the current form or business task. It can include
fields such as `title`, `width`, labels, and form items.

```toml
title = "Deploy"
show_header_title = false
width = 520
submit_label = "Deploy"
cancel_label = "Cancel"

[[items]]
type = "markdown"
content = "## Deploy checklist\nReview before continuing."

[[items]]
type = "input"
id = "branch"
label = "Branch"
default = "main"
required = true

[[items]]
type = "search"
id = "query"
label = "Search"
placeholder = "Type a keyword"
button_label = "Search"
required = true

[[items]]
type = "select"
id = "environment"
label = "Environment"
options = ["dev", "staging", "prod"]
default = "staging"

[[items]]
type = "checkbox"
id = "confirmed"
label = "I have reviewed the deployment"
required = true
```

Set `show_header_title = false` to hide the large heading inside the form
window while keeping the OS window title bar text.

Run it:

```bash
rune form --config deploy.toml
```

On submit, Rune prints one JSON object to stdout:

```json
{"branch":"main","environment":"staging","confirmed":true}
```

Supported form item types:

- `text`
- `markdown`
- `input`
- `search`
- `textarea`
- `select`
- `checkbox`

## Style Config

Rune loads style from the default global config path unless `--style <FILE>` is
provided.

Default global paths:

- macOS/Linux: `$XDG_CONFIG_HOME/rune/config.toml`
- macOS/Linux fallback: `~/.config/rune/config.toml`
- Windows: `%APPDATA%/rune/config.toml`

Create a default global style file:

```bash
rune config init
```

Print the active global style config:

```bash
rune config show
```

Print the global style path:

```bash
rune config path
```

Use a style file for one command:

```bash
rune form --config deploy.toml --style ./dark.toml
rune input "Name" --style ~/.config/rune/compact.toml
rune stream --style ./stream-style.toml -- cargo test
```

Example style file:

```toml
[window]
theme = "dark"
always_on_top = false
corner_radius = 8
shadow = true

[window.header]
height = 40
font_size = 16
show_icon = true

[window.body]
padding = 16
font_size = 14
line_height = 1.5
control_height = 36

[window.colors]
background = "#1e1e1e"
text = "#e0e0e0"
accent = "#5b8def"
border = "#333333"
```

`--config` and `--style` have separate responsibilities:

- `--config`: current form or task config, such as `title`, `width`, `items`,
  and `control_width`.
- `--style`: UI style config, such as `theme`, `padding`, `font_size`, and
  `control_height`.

CLI flags such as `--theme` and `--always-on-top` override style config for the
current command.

## Exit Codes

```text
0    success
1    cancelled
2    configuration or parse error
5    timed out
130  stopped
```

## Development

Format, test, and check whitespace before committing:

```bash
cargo fmt
cargo test
git diff --check
```

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

fn rune() -> Command {
    Command::new(env!("CARGO_BIN_EXE_rune"))
}

fn temp_home(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("rune-test-{name}-{nanos}"));
    fs::create_dir_all(&path).expect("failed to create temp home");
    path
}

fn run_with_home(home: &Path, args: &[&str]) -> Output {
    rune()
        .args(args)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("APPDATA", home.join("AppData").join("Roaming"))
        .output()
        .expect("failed to run rune")
}

#[cfg(not(windows))]
fn run_with_home_without_xdg(home: &Path, args: &[&str]) -> Output {
    rune()
        .args(args)
        .env("HOME", home)
        .env_remove("XDG_CONFIG_HOME")
        .output()
        .expect("failed to run rune")
}

#[cfg(windows)]
fn expected_global_config_path(home: &Path) -> PathBuf {
    home.join("AppData")
        .join("Roaming")
        .join("rune")
        .join("config.toml")
}

#[cfg(not(windows))]
fn expected_global_config_path(home: &Path) -> PathBuf {
    home.join(".config").join("rune").join("config.toml")
}

#[test]
fn missing_call_config_exits_two_and_keeps_stdout_empty() {
    let home = temp_home("missing-config");
    let output = run_with_home(&home, &["form", "--config", "does-not-exist.toml"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("failed to read config file"),
        "stderr was: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn invalid_call_config_exits_two_and_keeps_stdout_empty() {
    let home = temp_home("invalid-config");
    let config = home.join("bad.toml");
    fs::write(
        &config,
        r#"
        [[items]]
        type = "button"
        id = "ok"
        "#,
    )
    .expect("failed to write invalid config");

    let output = run_with_home(&home, &["form", "--config", config.to_str().unwrap()]);

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("failed to parse TOML config"),
        "stderr was: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn missing_explicit_style_exits_two_and_keeps_stdout_empty() {
    let home = temp_home("missing-style");
    let output = run_with_home(
        &home,
        &["form", "--style", "does-not-exist.toml", "--text", "Hello"],
    );

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("failed to read style config file"),
        "stderr was: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn invalid_explicit_style_exits_two_and_keeps_stdout_empty() {
    let home = temp_home("invalid-style");
    let style = home.join("bad-style.toml");
    fs::write(&style, "not = [valid").expect("failed to write invalid style");

    let output = run_with_home(
        &home,
        &[
            "form",
            "--style",
            style.to_str().unwrap(),
            "--text",
            "Hello",
        ],
    );

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("failed to parse TOML style config"),
        "stderr was: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn config_path_prints_rune_config_file() {
    let home = temp_home("path");
    let output = run_with_home(&home, &["config", "path"]);

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert_eq!(
        stdout.trim_end(),
        expected_global_config_path(&home).display().to_string()
    );
}

#[cfg(not(windows))]
#[test]
fn config_path_falls_back_to_home_config_without_xdg_config_home() {
    let home = temp_home("path-no-xdg");
    let output = run_with_home_without_xdg(&home, &["config", "path"]);

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert_eq!(
        stdout.trim_end(),
        home.join(".config")
            .join("rune")
            .join("config.toml")
            .display()
            .to_string()
    );
}

#[test]
fn config_show_prints_merged_style_config() {
    let home = temp_home("show");
    let config_path = expected_global_config_path(&home);
    fs::create_dir_all(
        config_path
            .parent()
            .expect("config path should have parent"),
    )
    .expect("failed to create config dir");
    fs::write(
        config_path,
        r#"
        [window]
        theme = "light"
        always_on_top = false
        "#,
    )
    .expect("failed to write style config");

    let output = run_with_home(
        &home,
        &["config", "show", "--theme", "dark", "--always-on-top"],
    );

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(stdout.contains("theme = \"dark\""));
    assert!(stdout.contains("always_on_top = true"));
}

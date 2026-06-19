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
        .output()
        .expect("failed to run rune")
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
fn config_path_prints_rune_config_file() {
    let home = temp_home("path");
    let output = run_with_home(&home, &["config", "path"]);

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(
        stdout.trim_end().ends_with("rune/config.toml"),
        "path was: {stdout}"
    );
}

#[test]
fn config_show_prints_merged_style_config() {
    let home = temp_home("show");
    let config_dir = home.join(".config").join("rune");
    fs::create_dir_all(&config_dir).expect("failed to create config dir");
    fs::write(
        config_dir.join("config.toml"),
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

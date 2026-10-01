use std::path::PathBuf;
use std::process::{Command, Output};

const CLIENT_ID: &str = "abcdefghijklmnopqrstuvwxyz012345";

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_perseus-cli"))
        .args(args)
        .env_remove("SOUNDCLOUD_CLIENT_ID")
        .env("NO_COLOR", "1")
        .output()
        .expect("executa perseus-cli")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).replace("\r\n", "\n")
}

fn golden(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join(name)
}

#[test]
fn help_matches_the_golden_file() {
    let output = cli(&["--help"]);
    assert!(output.status.success());
    let help = text(&output.stdout);
    let path = golden("help.txt");
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().expect("pasta")).expect("pasta golden");
        std::fs::write(&path, &help).expect("grava golden");
    }
    let expected = std::fs::read_to_string(&path)
        .expect("tests/golden/help.txt (UPDATE_GOLDEN=1 para gerar)")
        .replace("\r\n", "\n");
    assert_eq!(help, expected, "--help mudou");
}

#[test]
fn version_reports_the_workspace_version() {
    let output = cli(&["--version"]);
    assert!(output.status.success());
    assert_eq!(
        text(&output.stdout).trim(),
        format!("perseus-cli {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn hostile_urls_are_rejected_as_usage_errors() {
    let hostile = [
        "https://evil.example/a/b",
        "https://soundcloud.com.evil.com/a/b",
        "https://evil.com/?x=on.soundcloud.com",
        "https://soundcloud.com@evil.com/a/b",
        "https://soundcloud.com:8080/a/b",
        "javascript:alert(1)",
        "file:///C:/Windows/win.ini",
        "http://127.0.0.1/a/b",
        "https://soundcloud.com/../../etc/passwd",
        "https://soundcloud.com/%2e%2e/%2e%2e/x",
        "https://xn--soundcloud-xyz.com/a/b",
        "https://soundcloud.com/a/b%00c",
        "",
    ];
    for url in hostile {
        let output = cli(&["--client-id", CLIENT_ID, "--info", url]);
        assert_eq!(
            output.status.code(),
            Some(2),
            "{url:?} deveria ser recusada: {}",
            text(&output.stderr)
        );
    }
}

#[test]
fn invalid_options_are_usage_errors_before_any_request() {
    let cases: [&[&str]; 5] = [
        &["--name-template", "{artist}", "https://soundcloud.com/a/b"],
        &[
            "--name-template",
            "{nope} {title}",
            "https://soundcloud.com/a/b",
        ],
        &["--workers", "99", "https://soundcloud.com/a/b"],
        &[
            "--interval",
            "1",
            "--watch",
            "https://soundcloud.com/a/sets/b",
        ],
        &["--client-id", "curto", "https://soundcloud.com/a/b"],
    ];
    for args in cases {
        let output = cli(args);
        assert_eq!(
            output.status.code(),
            Some(2),
            "{args:?}: {}",
            text(&output.stderr)
        );
    }
}

#[test]
fn json_logs_are_structured_and_never_leak_the_client_id() {
    let dir = tempfile::tempdir().expect("tempdir");
    let log_file = dir.path().join("perseus.log");
    let output = cli(&[
        "--client-id",
        CLIENT_ID,
        "--log-format",
        "json",
        "--log-file",
        log_file.to_str().expect("caminho"),
        "--info",
        "https://evil.example/a/b",
    ]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = text(&output.stderr);
    let lines: Vec<&str> = stderr.lines().filter(|l| !l.trim().is_empty()).collect();
    assert!(!lines.is_empty(), "sem logs");
    for line in &lines {
        let value: serde_json::Value =
            serde_json::from_str(line).unwrap_or_else(|_| panic!("linha nao e JSON: {line}"));
        assert!(
            value.get("level").is_some() && value.get("timestamp").is_some(),
            "{line}"
        );
    }
    assert!(
        lines.iter().any(|line| line.contains("run_id")),
        "run_id ausente: {stderr}"
    );
    let file_log = std::fs::read_to_string(&log_file).unwrap_or_default();
    for sink in [&stderr, &text(&output.stdout), &file_log] {
        assert!(!sink.contains(CLIENT_ID), "client_id vazou no log");
    }
}

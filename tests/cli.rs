//! Testes ponta a ponta: executam o binário compilado como um usuário faria.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn dupfind(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_dupfind"))
        .args(args)
        .output()
        .expect("failed to run dupfind")
}

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("one.txt"), b"hello").unwrap();
    fs::write(dir.path().join("two.txt"), b"hello").unwrap();
    fs::write(dir.path().join("other.txt"), b"world").unwrap();
    dir
}

fn path_arg(path: &Path) -> &str {
    path.to_str().expect("temp dir path is UTF-8")
}

#[test]
fn json_output_is_machine_readable() {
    let dir = fixture();
    let output = dupfind(&["--format", "json", path_arg(dir.path())]);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["summary"]["scanned_files"], 3);
    assert_eq!(json["summary"]["duplicate_groups"], 1);
    assert_eq!(json["summary"]["reclaimable_bytes"], 5);
    assert_eq!(json["groups"][0]["size"], 5);
    assert_eq!(
        json["groups"][0]["hash"],
        blake3::hash(b"hello").to_hex().as_str()
    );
}

#[test]
fn text_output_lists_groups_and_summary() {
    let dir = fixture();
    let output = dupfind(&[path_arg(dir.path())]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.starts_with("2 copies of 5 B (5 B reclaimable)"));
    assert!(stdout.contains("one.txt"));
    assert!(stdout.contains("two.txt"));
    assert!(!stdout.contains("other.txt"));
    assert!(stdout.contains("Scanned 3 files: 1 duplicate group"));
}

#[test]
fn min_size_accepts_human_units() {
    let dir = fixture();
    let output = dupfind(&["--min-size", "1K", path_arg(dir.path())]);

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "Scanned 0 files: no duplicates found.\n"
    );
}

#[test]
fn missing_path_is_a_usage_error() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nope");
    let output = dupfind(&[path_arg(&missing)]);

    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("path not found"));
}

#[test]
fn invalid_min_size_is_rejected() {
    let output = dupfind(&["--min-size", "10X", "."]);

    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown unit"));
}

#[test]
fn zero_threads_is_rejected() {
    let output = dupfind(&["--threads", "0", "."]);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn version_flag_prints_the_crate_version() {
    let output = dupfind(&["--version"]);

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        format!("dupfind {}", env!("CARGO_PKG_VERSION"))
    );
}

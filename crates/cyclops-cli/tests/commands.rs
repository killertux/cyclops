//! Runs the real binary. The `Cli` tables in `specs/cli/commands.md` will take over once
//! temoc can check that output contains a text.

#![cfg(test)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn sample(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../samples")
        .join(name)
}

fn cyclops(folder: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cyclops"))
        .args(arguments)
        .current_dir(folder)
        .output()
        .expect("the binary runs")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn analyze_works_on_the_current_folder() {
    let output = cyclops(&sample("rust-shop"), &["analyze"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(text(&output.stdout).contains("6 files"));
    assert_eq!(text(&output.stderr), "");
}

#[test]
fn analyze_works_on_another_folder() {
    let output = cyclops(&sample("rust-shop"), &["-C", "src", "analyze"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(text(&output.stdout).contains("5 files"));
}

#[test]
fn a_folder_that_does_not_exist_fails() {
    let output = cyclops(&sample("rust-shop"), &["-C", "nowhere", "analyze"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(text(&output.stdout), "");
    assert!(text(&output.stderr).contains("Project not found: nowhere"));
}

#[test]
fn an_unknown_command_fails() {
    let output = cyclops(&sample("rust-shop"), &["nonsense"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(text(&output.stdout), "");
    assert!(text(&output.stderr).contains("nonsense"));
}

#[test]
fn file_problems_are_reported_but_do_not_fail() {
    let output = cyclops(&sample("broken"), &["analyze"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(text(&output.stderr).contains("src/bad.rs: syntax error"));
}

#[test]
fn nothing_to_analyze_is_not_an_error() {
    let output = cyclops(&sample("docs-only"), &["analyze"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(text(&output.stderr).contains("No source files found."));
}

#[test]
fn report_prints_the_metrics_of_symbols_and_files() {
    let output = cyclops(&sample("rust-shop"), &["report"]);
    let stdout = text(&output.stdout);
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout.contains("shop::orders::Order::total  cyclomatic=3 cognitive=3"));
    assert!(stdout.contains("src/orders.rs  lines="));
}

#[test]
fn test_code_is_only_reported_when_asked_for() {
    let hidden = text(&cyclops(&sample("rust-shop"), &["report"]).stdout);
    let shown = text(&cyclops(&sample("rust-shop"), &["report", "--tests"]).stdout);
    assert!(!hidden.contains("says_hello"));
    assert!(shown.contains("shop::notes::tests::says_hello"));
    assert!(shown.contains("tests/orders::starts_empty"));
}

/// A long chain reads flat but parses into a very deep tree.
#[test]
fn a_very_long_expression_does_not_crash_the_analysis() {
    let folder = std::env::temp_dir().join(format!("cyclops-cli-{}", std::process::id()));
    std::fs::create_dir_all(&folder).expect("a folder");
    let terms = vec!["a"; 100_000].join(" + ");
    let chain = "if (a) {} else ".repeat(10_000);
    let rust_chain = "if a {} else ".repeat(10_000);
    let files = [
        (
            "sum.ts",
            format!("function f(a: number) {{ return {terms}; }}\n"),
        ),
        (
            "chain.ts",
            format!("function g(a: boolean) {{ {chain} {{}} }}\n"),
        ),
        ("sum.rs", format!("fn f(a: u8) -> u8 {{ {terms} }}\n")),
        (
            "chain.rs",
            format!("fn g(a: bool) {{ {rust_chain} {{}} }}\n"),
        ),
    ];
    for (name, text) in &files {
        std::fs::write(folder.join(name), text).expect("a file");
    }

    let output = cyclops(&folder, &["analyze"]);

    std::fs::remove_dir_all(&folder).expect("the folder is removed");
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert!(text(&output.stdout).contains("4 files"));
}

#[test]
fn a_typescript_project_is_analyzed_too() {
    let output = cyclops(&sample("ts-shop"), &["report"]);
    let stdout = text(&output.stdout);
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout.contains("src/orders::Order::total  cyclomatic=3 cognitive=3"));
    assert!(!stdout.contains("startsEmpty"));
}

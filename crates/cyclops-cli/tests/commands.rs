//! Runs the real binary. The `Cli` tables in `specs/cli/commands.md` will take over once
//! temoc can check that output contains a text.

#![cfg(test)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A copy of a sample, because analyzing a project writes its cache into it.
struct Copy(PathBuf);

impl Copy {
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Copy {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).expect("the copy is removed");
    }
}

fn copy_folder(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("a folder");
    for entry in std::fs::read_dir(from).expect("a folder") {
        let entry = entry.expect("an entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("a type").is_dir() {
            copy_folder(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).expect("copied");
        }
    }
}

fn sample(name: &str) -> Copy {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let target = std::env::temp_dir()
        .join(format!("cyclops-cli-{}-{id}", std::process::id()))
        .join(name);
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../samples")
        .join(name);
    copy_folder(&source, &target);
    Copy(target)
}

/// Runs the binary with no settings of your own.
fn cyclops(folder: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cyclops"))
        .args(arguments)
        .current_dir(folder)
        .env_clear()
        .env("HOME", folder)
        .output()
        .expect("the binary runs")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn analyze_works_on_the_current_folder() {
    let output = cyclops(sample("rust-shop").path(), &["analyze"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(text(&output.stdout).contains("6 files"));
    assert_eq!(text(&output.stderr), "");
}

#[test]
fn analyze_works_on_another_folder() {
    let output = cyclops(sample("rust-shop").path(), &["-C", "src", "analyze"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(text(&output.stdout).contains("5 files"));
}

#[test]
fn a_folder_that_does_not_exist_fails() {
    let output = cyclops(sample("rust-shop").path(), &["-C", "nowhere", "analyze"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(text(&output.stdout), "");
    assert!(text(&output.stderr).contains("Project not found: nowhere"));
}

#[test]
fn an_unknown_command_fails() {
    let output = cyclops(sample("rust-shop").path(), &["nonsense"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(text(&output.stdout), "");
    assert!(text(&output.stderr).contains("nonsense"));
}

#[test]
fn file_problems_are_reported_but_do_not_fail() {
    let output = cyclops(sample("broken").path(), &["analyze"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(text(&output.stderr).contains("src/bad.rs: syntax error"));
}

#[test]
fn nothing_to_analyze_is_not_an_error() {
    let output = cyclops(sample("docs-only").path(), &["analyze"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(text(&output.stderr).contains("No source files found."));
}

#[test]
fn report_prints_the_metrics_of_symbols_and_files() {
    let output = cyclops(sample("rust-shop").path(), &["report"]);
    let stdout = text(&output.stdout);
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout.contains("shop::orders::Order::total  cyclomatic=3 cognitive=3"));
    assert!(
        stdout.contains("src/orders.rs  cyclomatic=4 lines="),
        "{stdout}"
    );
}

#[test]
fn test_code_is_only_reported_when_asked_for() {
    let hidden = text(&cyclops(sample("rust-shop").path(), &["report"]).stdout);
    let shown = text(&cyclops(sample("rust-shop").path(), &["report", "--tests"]).stdout);
    assert!(!hidden.contains("says_hello"));
    assert!(shown.contains("shop::notes::tests::says_hello"));
    assert!(shown.contains("tests/orders::starts_empty"));
}

/// A long chain reads flat but parses into a very deep tree.
#[test]
fn a_very_long_expression_does_not_crash_the_analysis() {
    let folder = std::env::temp_dir().join(format!("cyclops-cli-deep-{}", std::process::id()));
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
    let output = cyclops(sample("ts-shop").path(), &["report"]);
    let stdout = text(&output.stdout);
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout.contains("src/orders::Order::total  cyclomatic=3 cognitive=3"));
    assert!(!stdout.contains("startsEmpty"));
}

#[test]
fn a_second_analysis_takes_everything_from_the_cache() {
    let project = sample("rust-shop");
    let first = text(&cyclops(project.path(), &["analyze"]).stdout);
    let second = text(&cyclops(project.path(), &["analyze"]).stdout);
    assert!(first.contains("6 parsed"), "{first}");
    assert!(second.contains("0 parsed"), "{second}");
}

#[test]
fn init_writes_the_languages_once() {
    let project = sample("mixed");
    let first = cyclops(project.path(), &["init"]);
    let config =
        std::fs::read_to_string(project.path().join(".cyclops/config.toml")).expect("written");
    let second = cyclops(project.path(), &["init"]);
    assert_eq!(first.status.code(), Some(0));
    assert!(
        config.contains("languages = [\"javascript\", \"rust\", \"typescript\"]"),
        "{config}"
    );
    assert!(text(&second.stdout).contains("Already initialized"));
}

#[test]
fn bad_settings_stop_the_analysis() {
    let project = sample("rust-shop");
    std::fs::create_dir_all(project.path().join(".cyclops")).expect("a folder");
    std::fs::write(
        project.path().join(".cyclops/config.toml"),
        "[tests]\ninclude = \"maybe\"\n",
    )
    .expect("written");
    let output = cyclops(project.path(), &["analyze"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(text(&output.stderr).contains("tests.include must be true or false."));
}

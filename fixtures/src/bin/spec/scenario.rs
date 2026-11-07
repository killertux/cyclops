//! Takes the steps of a row, as `specs/writing-specs.md` describes them.

use crate::scratch::Scratch;
use cyclops_application::analysis::ProjectAnalysis;
use cyclops_application::ports::{FileSource, LanguageAdapter, Linked, SourceFile};
use cyclops_cli::{Environment, adapters, analyze_with, flag_layer, settings, user_config_path};
use cyclops_domain::facts::FileFacts;
use cyclops_domain::language::LanguageId;
use cyclops_domain::path::RelativePath;
use cyclops_domain::settings::{Layer, SettingKey, SettingKind, Settings};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};
use toml_edit::{Array, ArrayOfTables, DocumentMut, Item, Table, value};

/// A small valid source file, for the `add` step.
const SMALL_FILE: &str = "// Added by a step.\n";
const PROJECT_CONFIG: &str = ".cyclops/config.toml";
const CACHE: &str = ".cyclops/cache/cyclops.db";

/// An adapter whose version went up, for the `upgrade LANGUAGE adapter` step.
struct Upgraded(Box<dyn LanguageAdapter>);

impl LanguageAdapter for Upgraded {
    fn name(&self) -> &'static str {
        self.0.name()
    }

    fn version(&self) -> u32 {
        self.0.version() + 1
    }

    fn language_of(&self, path: &RelativePath) -> Option<LanguageId> {
        self.0.language_of(path)
    }

    fn extract(&self, source: &SourceFile<'_>) -> FileFacts {
        self.0.extract(source)
    }

    fn link(&self, facts: &[&FileFacts], source: &dyn FileSource) -> Linked {
        self.0.link(facts, source)
    }
}

pub struct Scenario {
    pub scratch: Scratch,
    variables: HashMap<String, String>,
    tests_flag: Option<bool>,
    upgraded: Vec<String>,
}

fn failure(step: &str, error: impl std::fmt::Display) -> String {
    format!("{step}: {error}")
}

/// A value the way `set` writes it to a file: as the setting expects it when the setting
/// is known, so that a wrong value is wrong on purpose.
fn toml_value(key: &str, text: &str) -> Item {
    let kind = SettingKey::parse(key).map(SettingKey::kind);
    if kind == Some(SettingKind::List) {
        let mut array = Array::new();
        for item in text.split(',').map(str::trim) {
            array.push(item);
        }
        return value(array);
    }
    if let Ok(flag) = text.parse::<bool>() {
        return value(flag);
    }
    if let Ok(number) = text.parse::<i64>() {
        return value(number);
    }
    value(text)
}

/// Splits `METRIC SCOPE WHEN SEVERITY [in PATTERN] [disabled] [named NAME]`.
fn alert_table(rule: &str) -> Result<Table, String> {
    let mut rest = rule.trim();
    let mut table = Table::new();
    if let Some((before, name)) = rest.rsplit_once(" named ") {
        table.insert("name", value(name.trim()));
        rest = before.trim();
    }
    if let Some(before) = rest.strip_suffix(" disabled") {
        table.insert("enabled", value(false));
        rest = before.trim();
    }
    if let Some((before, pattern)) = rest.rsplit_once(" in ") {
        let mut paths = Array::new();
        paths.push(pattern.trim());
        table.insert("paths", value(paths));
        rest = before.trim();
    }
    let words: Vec<&str> = rest.split_whitespace().collect();
    let (Some(metric), Some(scope), Some(severity)) = (words.first(), words.get(1), words.last())
    else {
        return Err(format!("not a rule: {rule}"));
    };
    let when = words
        .get(2..words.len().saturating_sub(1))
        .unwrap_or_default();
    table.insert("metric", value(*metric));
    table.insert("scope", value(*scope));
    table.insert("when", value(when.join(" ")));
    table.insert("severity", value(*severity));
    Ok(table)
}

impl Scenario {
    pub fn new(scratch: Scratch) -> Self {
        let mut variables = HashMap::new();
        variables.insert("HOME".to_owned(), scratch.home().display().to_string());
        Self {
            scratch,
            variables,
            tests_flag: None,
            upgraded: Vec::new(),
        }
    }

    pub fn root(&self) -> &Path {
        self.scratch.root()
    }

    pub fn environment(&self) -> Environment {
        Environment::new(self.variables.clone())
    }

    pub fn variables(&self) -> &HashMap<String, String> {
        &self.variables
    }

    pub fn flags(&self) -> Layer {
        flag_layer(self.tests_flag)
    }

    /// Analyzes the project, the way the final analysis of a row does.
    pub fn analyze(&self) -> Result<ProjectAnalysis, String> {
        let adapters: Vec<Box<dyn LanguageAdapter>> = adapters()
            .into_iter()
            .map(|adapter| {
                if self.upgraded.iter().any(|name| name == adapter.name()) {
                    Box::new(Upgraded(adapter)) as Box<dyn LanguageAdapter>
                } else {
                    adapter
                }
            })
            .collect();
        analyze_with(self.root(), &self.environment(), &self.flags(), adapters)
            .map_err(|error| error.to_string())
    }

    pub fn settings(&self) -> Result<Settings, String> {
        settings(self.root(), &self.environment(), &self.flags(), true).map_err(|e| e.to_string())
    }

    fn user_config(&self) -> Result<PathBuf, String> {
        user_config_path(&self.environment()).ok_or_else(|| "no home folder".to_owned())
    }

    fn project_config(&self) -> PathBuf {
        self.root().join(PROJECT_CONFIG)
    }

    fn edit_toml(
        path: &Path,
        change: impl FnOnce(&mut DocumentMut) -> Result<(), String>,
    ) -> Result<(), String> {
        let text = fs::read_to_string(path).unwrap_or_default();
        let mut document: DocumentMut = text
            .parse()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        change(&mut document)?;
        if let Some(folder) = path.parent() {
            fs::create_dir_all(folder).map_err(|e| e.to_string())?;
        }
        fs::write(path, document.to_string()).map_err(|e| e.to_string())
    }

    fn set(path: &Path, key: &str, text: &str) -> Result<(), String> {
        Self::edit_toml(path, |document| {
            let mut names: Vec<&str> = key.split('.').collect();
            let last = names.pop().ok_or("an empty setting")?;
            let mut table = document.as_table_mut();
            for name in names {
                let entry = table.entry(name).or_insert_with(|| {
                    let mut inner = Table::new();
                    inner.set_implicit(true);
                    Item::Table(inner)
                });
                table = entry
                    .as_table_mut()
                    .ok_or_else(|| format!("{name} is not a table"))?;
            }
            table.insert(last, toml_value(key, text));
            Ok(())
        })
    }

    fn add_alert(path: &Path, rule: &str) -> Result<(), String> {
        let table = alert_table(rule)?;
        Self::edit_toml(path, |document| {
            let alerts = document
                .entry("alerts")
                .or_insert_with(|| Item::ArrayOfTables(ArrayOfTables::new()));
            alerts
                .as_array_of_tables_mut()
                .ok_or("alerts is not a list of tables")?
                .push(table);
            Ok(())
        })
    }

    fn append(path: &Path, text: &str) -> Result<(), String> {
        let mut existing = fs::read_to_string(path).unwrap_or_default();
        existing.push_str(text);
        if let Some(folder) = path.parent() {
            fs::create_dir_all(folder).map_err(|e| e.to_string())?;
        }
        fs::write(path, existing).map_err(|e| e.to_string())
    }

    fn git(&self, arguments: &[&str]) -> Result<(), String> {
        let output = Command::new("git")
            .args(arguments)
            .current_dir(self.root())
            .env("GIT_AUTHOR_NAME", "Spec")
            .env("GIT_AUTHOR_EMAIL", "spec@example.com")
            .env("GIT_COMMITTER_NAME", "Spec")
            .env("GIT_COMMITTER_EMAIL", "spec@example.com")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .map_err(|e| e.to_string())?;
        if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).into_owned())
        }
    }

    /// Takes every step of a row, separated by semicolons.
    pub fn steps(
        &mut self,
        steps: &str,
        cyclops: &dyn Fn(&Self, &str) -> Result<(), String>,
    ) -> Result<(), String> {
        for step in steps
            .split(';')
            .map(str::trim)
            .filter(|step| !step.is_empty())
        {
            self.step(step, cyclops)?;
        }
        Ok(())
    }

    fn step(
        &mut self,
        step: &str,
        cyclops: &dyn Fn(&Self, &str) -> Result<(), String>,
    ) -> Result<(), String> {
        let root = self.root().to_path_buf();
        let fail = |error: String| failure(step, error);
        let io = |error: std::io::Error| failure(step, error);
        if let Some(arguments) = step.strip_prefix("cyclops ") {
            return cyclops(self, arguments).map_err(fail);
        }
        if let Some(rest) = step.strip_prefix("set user ") {
            let (key, text) = rest.split_once(' ').ok_or("set needs a value")?;
            return Self::set(&self.user_config()?, key, text).map_err(fail);
        }
        if let Some(rest) = step.strip_prefix("set ") {
            let (key, text) = rest.split_once(' ').ok_or("set needs a value")?;
            return Self::set(&self.project_config(), key, text).map_err(fail);
        }
        if let Some(rule) = step.strip_prefix("add user alert ") {
            return Self::add_alert(&self.user_config()?, rule).map_err(fail);
        }
        if let Some(rule) = step.strip_prefix("add alert ") {
            return Self::add_alert(&self.project_config(), rule).map_err(fail);
        }
        if let Some(text) = step.strip_prefix("add config comment ") {
            return Self::append(&self.project_config(), &format!("# {text}\n")).map_err(fail);
        }
        if let Some(rest) = step.strip_prefix("env ") {
            let (name, text) = rest.split_once(' ').ok_or("env needs a value")?;
            let text = if name == "XDG_CONFIG_HOME" && !Path::new(text).is_absolute() {
                self.scratch.home().join(text).display().to_string()
            } else {
                text.to_owned()
            };
            self.variables.insert(name.to_owned(), text);
            return Ok(());
        }
        if let Some(flag) = step.strip_prefix("flag ") {
            self.tests_flag = match flag {
                "--tests" => Some(true),
                "--no-tests" => Some(false),
                other => return Err(fail(format!("unknown flag {other}"))),
            };
            return Ok(());
        }
        if let Some(language) = step
            .strip_prefix("upgrade ")
            .and_then(|rest| rest.strip_suffix(" adapter"))
        {
            self.upgraded.push(language.to_owned());
            return Ok(());
        }
        let done = match step {
            "analyze" => return self.analyze().map(drop).map_err(fail),
            "git init" => {
                self.git(&["init", "-q"])?;
                self.git(&["add", "-A"])?;
                return self.git(&["commit", "-q", "-m", "First"]).map_err(fail);
            }
            "break config" => fs::create_dir_all(root.join(".cyclops"))
                .and_then(|()| fs::write(self.project_config(), "this is [not toml")),
            "break user config" => {
                let path = self.user_config()?;
                fs::create_dir_all(path.parent().unwrap_or(&root))
                    .and_then(|()| fs::write(path, "this is [not toml"))
            }
            "delete cache" => fs::remove_dir_all(root.join(".cyclops/cache")),
            "corrupt cache" => fs::write(root.join(CACHE), "not a database, only garbage"),
            "outdate cache" => return outdate(&root.join(CACHE)).map_err(fail),
            _ => return self.file_step(step).map_err(io),
        };
        done.map_err(io)
    }

    fn file_step(&self, step: &str) -> std::io::Result<()> {
        let root = self.root();
        let unknown = || std::io::Error::other(format!("unknown step: {step}"));
        let (verb, rest) = step.split_once(' ').ok_or_else(unknown)?;
        match verb {
            "add" => match rest.strip_prefix("binary ") {
                Some(path) => self.scratch.write(path, &[0xff, 0xfe, 0x00]),
                None => self.scratch.write(rest, SMALL_FILE.as_bytes()),
            },
            "edit" => {
                let path = root.join(rest);
                let mut text = fs::read_to_string(&path)?;
                text.push_str("// Edited by a step.\n");
                fs::write(path, text)
            }
            "touch" => {
                let later = SystemTime::now() + Duration::from_secs(5);
                fs::File::options()
                    .write(true)
                    .open(root.join(rest))?
                    .set_modified(later)
            }
            "delete" => fs::remove_file(root.join(rest)),
            "lock" => self.scratch.lock(rest),
            "ignore" => {
                let path = root.join(".gitignore");
                let mut text = fs::read_to_string(&path).unwrap_or_default();
                text.push_str(rest);
                text.push('\n');
                fs::write(path, text)
            }
            _ => Err(unknown()),
        }
    }
}

/// Gives the cache an older schema version.
fn outdate(path: &Path) -> Result<(), String> {
    let connection = rusqlite::Connection::open(path).map_err(|e| e.to_string())?;
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|e| e.to_string())?;
    connection
        .pragma_update(None, "user_version", version - 1)
        .map_err(|e| e.to_string())
}

/// Steps that only make sense for the `Cli` fixture.
pub fn no_cyclops(_: &Scenario, arguments: &str) -> Result<(), String> {
    Err(format!(
        "cyclops {arguments} is only a step for the Cli fixture"
    ))
}

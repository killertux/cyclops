//! An alert rule being written, edited like a buffer: one field at a time, checked as
//! it is typed. See `specs/tui/alerts.md`.

use crate::input::Key;
use cyclops_domain::alert::AlertRule;
use cyclops_domain::glob::Glob;
use cyclops_domain::metric::{MetricId, Scope};
use cyclops_domain::settings::{AlertSource, Origin, RawValue, Severity};

/// The fields of a rule, in order.
pub const FIELDS: [&str; 7] = [
    "name", "metric", "scope", "when", "severity", "paths", "enabled",
];

/// The field a new rule starts on.
const WHEN: usize = 3;

/// A rule being written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleEditor {
    /// The value of each field, as text, in the order of [`FIELDS`].
    pub values: [String; 7],
    /// The field the cursor is on.
    pub field: usize,
    /// What is being typed into the field, until it is kept or dropped.
    pub typing: Option<String>,
    /// The rule it changes: whether it is yours, and its place in its file.
    pub existing: Option<(bool, usize)>,
    /// Why the rule cannot work, as it stands.
    pub error: Option<String>,
    /// What it was started from, such as a symbol.
    pub from: String,
}

/// What a key did to the editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edited {
    /// The editor used the key.
    Done,
    /// The editor has no use for it.
    Unused,
}

impl RuleEditor {
    /// A new rule from a symbol: its metric and scope, and a threshold the symbol meets.
    #[must_use]
    pub fn starting_from(from: &str, metric: MetricId, scope: Scope, value: Option<f64>) -> Self {
        let threshold = metric.descriptor().format(value.unwrap_or(0.0));
        let mut editor = Self {
            values: [
                String::new(),
                metric.name().to_owned(),
                scope.name().to_owned(),
                format!(">= {threshold}"),
                "warning".to_owned(),
                String::new(),
                "true".to_owned(),
            ],
            field: WHEN,
            typing: None,
            existing: None,
            error: None,
            from: from.to_owned(),
        };
        editor.check();
        editor
    }

    /// An existing rule, to change.
    #[must_use]
    pub fn of(rule: &AlertRule) -> Self {
        let severity = if rule.severity == Severity::Error {
            "error"
        } else {
            "warning"
        };
        let paths: Vec<&str> = rule.paths.iter().map(Glob::as_str).collect();
        let mut editor = Self {
            values: [
                rule.name.clone().unwrap_or_default(),
                rule.metric.name().to_owned(),
                rule.scope.name().to_owned(),
                rule.when.clone(),
                severity.to_owned(),
                paths.join(", "),
                rule.enabled.to_string(),
            ],
            field: WHEN,
            typing: None,
            existing: Some((rule.origin == Origin::UserFile, rule.position)),
            error: None,
            from: rule.title(),
        };
        editor.check();
        editor
    }

    fn value(&self, field: &str) -> &str {
        FIELDS
            .iter()
            .position(|name| *name == field)
            .and_then(|index| self.values.get(index))
            .map_or("", String::as_str)
    }

    /// The field the cursor is on.
    #[must_use]
    pub fn field_name(&self) -> &'static str {
        FIELDS.get(self.field).copied().unwrap_or("when")
    }

    /// The rule as `METRIC SCOPE WHEN SEVERITY`.
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "{} {} {} {}",
            self.value("metric"),
            self.value("scope"),
            self.value("when"),
            self.value("severity")
        )
    }

    /// The fields a settings file gets, leaving out the empty ones.
    #[must_use]
    pub fn fields(&self) -> Vec<(String, RawValue)> {
        FIELDS
            .iter()
            .zip(&self.values)
            .filter(|(_, value)| !value.trim().is_empty())
            .map(|(name, value)| {
                let value = value.trim();
                let raw = match *name {
                    "paths" => RawValue::List(
                        value
                            .split(',')
                            .map(str::trim)
                            .filter(|pattern| !pattern.is_empty())
                            .map(|pattern| RawValue::Text(pattern.to_owned()))
                            .collect(),
                    ),
                    "enabled" => match value {
                        "true" => RawValue::Bool(true),
                        "false" => RawValue::Bool(false),
                        other => RawValue::Text(other.to_owned()),
                    },
                    _ => RawValue::Text(value.to_owned()),
                };
                ((*name).to_owned(), raw)
            })
            .collect()
    }

    /// The rule, if it can work.
    ///
    /// # Errors
    ///
    /// Says why it cannot, as an invalid rule in a settings file would.
    pub fn rule(&self) -> Result<AlertRule, String> {
        let origin = match self.existing {
            Some((true, _)) => Origin::UserFile,
            _ => Origin::ProjectFile,
        };
        AlertRule::parse(&AlertSource {
            fields: self.fields(),
            origin,
            position: self.existing.map_or(0, |(_, position)| position),
        })
        .map_err(|error| error.to_string())
    }

    fn check(&mut self) {
        self.error = self.rule().err();
    }

    /// Takes a key: `i` edits the field, `Tab` moves to the next one, and while typing,
    /// `Enter` keeps what was typed and `Esc` drops it.
    pub fn key(&mut self, key: Key) -> Edited {
        let Some(mut typed) = self.typing.take() else {
            return match key {
                Key::Char('i') => {
                    self.typing = self.values.get(self.field).cloned();
                    Edited::Done
                }
                Key::Tab => {
                    self.field = (self.field + 1) % FIELDS.len();
                    Edited::Done
                }
                _ => Edited::Unused,
            };
        };
        match key {
            Key::Esc => {}
            Key::Enter => {
                if let Some(value) = self.values.get_mut(self.field) {
                    typed.trim().clone_into(value);
                }
                self.check();
            }
            Key::Backspace => {
                typed.pop();
                self.typing = Some(typed);
            }
            Key::Ctrl('u') => self.typing = Some(String::new()),
            Key::Char(character) => {
                typed.push(character);
                self.typing = Some(typed);
            }
            _ => self.typing = Some(typed),
        }
        Edited::Done
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(editor: &mut RuleEditor, keys: &str) {
        for character in keys.chars() {
            editor.key(Key::Char(character));
        }
    }

    #[test]
    fn a_new_rule_fires_on_what_it_started_from() {
        let editor =
            RuleEditor::starting_from("total", MetricId::Cyclomatic, Scope::Function, Some(3.0));
        assert_eq!(editor.summary(), "cyclomatic function >= 3 warning");
        assert_eq!(editor.field_name(), "when");
        assert_eq!(editor.error, None);
        assert!(
            editor
                .rule()
                .is_ok_and(|rule| rule.fires(3.0) && !rule.fires(2.0))
        );
    }

    #[test]
    fn typing_is_kept_on_enter_and_dropped_on_esc() {
        let mut editor =
            RuleEditor::starting_from("total", MetricId::Cyclomatic, Scope::Function, Some(3.0));
        editor.key(Key::Char('i'));
        editor.key(Key::Ctrl('u'));
        typed(&mut editor, "> 1");
        editor.key(Key::Esc);
        assert_eq!(editor.summary(), "cyclomatic function >= 3 warning");
        editor.key(Key::Char('i'));
        editor.key(Key::Ctrl('u'));
        typed(&mut editor, "around 1");
        editor.key(Key::Enter);
        assert_eq!(editor.summary(), "cyclomatic function around 1 warning");
        assert_eq!(editor.error.as_deref(), Some("Invalid threshold: around 1"));
    }

    #[test]
    fn tab_moves_through_the_fields_and_wraps() {
        let mut editor =
            RuleEditor::starting_from("total", MetricId::Cyclomatic, Scope::Function, Some(3.0));
        editor.key(Key::Tab);
        assert_eq!(editor.field_name(), "severity");
        for _ in 0..4 {
            editor.key(Key::Tab);
        }
        assert_eq!(editor.field_name(), "metric");
    }

    #[test]
    fn empty_fields_are_left_out_and_paths_are_a_list() {
        let mut editor =
            RuleEditor::starting_from("total", MetricId::Cyclomatic, Scope::Function, Some(3.0));
        editor.field = 5;
        editor.key(Key::Char('i'));
        typed(&mut editor, "src/a/**, src/b/**");
        editor.key(Key::Enter);
        let fields = editor.fields();
        assert!(!fields.iter().any(|(name, _)| name == "name"));
        assert!(fields.contains(&(
            "paths".to_owned(),
            RawValue::List(vec![
                RawValue::Text("src/a/**".to_owned()),
                RawValue::Text("src/b/**".to_owned()),
            ])
        )));
        assert!(fields.contains(&("enabled".to_owned(), RawValue::Bool(true))));
    }
}

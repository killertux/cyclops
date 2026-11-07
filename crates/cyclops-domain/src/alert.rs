//! Alert rules: thresholds that matter to a project. See `specs/explore/alerts.md`.

use crate::filter::Comparison;
use crate::glob::Glob;
use crate::metric::{MetricId, Scope};
use crate::settings::{AlertSource, Origin, RawValue, SettingsError, Severity};

#[derive(Debug, Clone, PartialEq)]
pub struct AlertRule {
    pub name: Option<String>,
    pub metric: MetricId,
    pub scope: Scope,
    pub comparison: Comparison,
    pub threshold: f64,
    pub severity: Severity,
    pub paths: Vec<Glob>,
    pub enabled: bool,
    pub origin: Origin,
}

fn text<'a>(source: &'a AlertSource, field: &str) -> Result<Option<&'a str>, SettingsError> {
    match source.fields.iter().find(|(name, _)| name == field) {
        None => Ok(None),
        Some((_, RawValue::Text(value))) => Ok(Some(value)),
        Some(_) => Err(SettingsError::AlertField(format!(
            "alerts.{field} must be text."
        ))),
    }
}

fn required<'a>(source: &'a AlertSource, field: &str) -> Result<&'a str, SettingsError> {
    text(source, field)?
        .ok_or_else(|| SettingsError::AlertField(format!("An alert rule needs a {field}.")))
}

impl AlertRule {
    /// Checks a rule as a settings file wrote it.
    ///
    /// # Errors
    ///
    /// Fails, and says why, for a rule that can never work.
    pub fn parse(source: &AlertSource) -> Result<Self, SettingsError> {
        let metric_name = required(source, "metric")?;
        let metric = MetricId::parse(metric_name)
            .ok_or_else(|| SettingsError::AlertField(format!("Unknown metric: {metric_name}")))?;
        let scope_name = required(source, "scope")?;
        let scope = Scope::parse(scope_name).ok_or_else(|| {
            SettingsError::AlertField(format!(
                "Unknown scope: {scope_name}. Use function, type, file or module."
            ))
        })?;
        if !metric.descriptor().applies_to(scope) {
            return Err(SettingsError::AlertField(format!(
                "{metric_name} does not apply to {scope_name}."
            )));
        }
        let when = required(source, "when")?;
        let (comparison, threshold) = Comparison::parse_threshold(when)
            .ok_or_else(|| SettingsError::AlertField(format!("Invalid threshold: {when}")))?;
        let severity = match required(source, "severity")? {
            "warning" => Severity::Warning,
            "error" => Severity::Error,
            other => {
                return Err(SettingsError::AlertField(format!(
                    "Unknown severity: {other}. Use warning or error."
                )));
            }
        };
        let paths = match source.fields.iter().find(|(name, _)| name == "paths") {
            None => Vec::new(),
            Some((_, RawValue::List(items))) => items
                .iter()
                .map(|item| match item {
                    RawValue::Text(pattern) => Ok(Glob::new(pattern)),
                    _ => Err(SettingsError::AlertField(
                        "alerts.paths must be a list of text.".to_owned(),
                    )),
                })
                .collect::<Result<_, _>>()?,
            Some(_) => {
                return Err(SettingsError::AlertField(
                    "alerts.paths must be a list of text.".to_owned(),
                ));
            }
        };
        let enabled = match source.fields.iter().find(|(name, _)| name == "enabled") {
            None => true,
            Some((_, RawValue::Bool(enabled))) => *enabled,
            Some(_) => {
                return Err(SettingsError::AlertField(
                    "alerts.enabled must be true or false.".to_owned(),
                ));
            }
        };
        Ok(Self {
            name: text(source, "name")?.map(ToOwned::to_owned),
            metric,
            scope,
            comparison,
            threshold,
            severity,
            paths,
            enabled,
            origin: source.origin,
        })
    }

    /// Checks every rule, and keeps the project's rule when a rule in each file has the
    /// same name.
    ///
    /// # Errors
    ///
    /// Fails on the first rule that can never work.
    pub fn all(sources: &[AlertSource]) -> Result<Vec<Self>, SettingsError> {
        let rules = sources
            .iter()
            .map(Self::parse)
            .collect::<Result<Vec<_>, _>>()?;
        let project_names: Vec<String> = rules
            .iter()
            .filter(|rule| rule.origin == Origin::ProjectFile)
            .filter_map(|rule| rule.name.clone())
            .collect();
        Ok(rules
            .into_iter()
            .filter(|rule| {
                rule.origin == Origin::ProjectFile
                    || rule
                        .name
                        .as_ref()
                        .is_none_or(|name| !project_names.contains(name))
            })
            .collect())
    }

    /// Whether a value breaks the rule.
    #[must_use]
    pub fn fires(&self, value: f64) -> bool {
        self.comparison.holds(value, self.threshold)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(fields: &[(&str, &str)], origin: Origin) -> AlertSource {
        AlertSource {
            fields: fields
                .iter()
                .map(|(name, value)| ((*name).to_owned(), RawValue::Text((*value).to_owned())))
                .collect(),
            origin,
        }
    }

    fn rule(metric: &str, scope: &str, when: &str, severity: &str) -> Result<AlertRule, String> {
        let fields = [
            ("metric", metric),
            ("scope", scope),
            ("when", when),
            ("severity", severity),
        ];
        AlertRule::parse(&source(&fields, Origin::ProjectFile)).map_err(|error| error.to_string())
    }

    #[test]
    fn a_rule_that_can_never_work_says_why() {
        assert_eq!(
            rule("nonsense", "function", "> 1", "warning")
                .err()
                .as_deref(),
            Some("Unknown metric: nonsense")
        );
        assert_eq!(
            rule("lcom", "function", "> 1", "warning").err().as_deref(),
            Some("lcom does not apply to function.")
        );
        assert_eq!(
            rule("cyclomatic", "function", "around 1", "warning")
                .err()
                .as_deref(),
            Some("Invalid threshold: around 1")
        );
        assert_eq!(
            rule("cyclomatic", "function", "> 1", "loud")
                .err()
                .as_deref(),
            Some("Unknown severity: loud. Use warning or error.")
        );
    }

    #[test]
    fn a_rolled_up_metric_applies_to_modules() {
        assert!(rule("cyclomatic", "module", "> 2", "warning").is_ok());
    }

    #[test]
    fn the_project_wins_a_name() {
        let fields = |severity| {
            vec![
                ("metric", "cyclomatic"),
                ("scope", "function"),
                ("when", "> 2"),
                ("severity", severity),
                ("name", "Complex"),
            ]
        };
        let rules = AlertRule::all(&[
            source(&fields("warning"), Origin::UserFile),
            source(&fields("error"), Origin::ProjectFile),
        ])
        .expect("valid");
        assert_eq!(rules.len(), 1);
        assert_eq!(
            rules.first().map(|rule| rule.severity),
            Some(Severity::Error)
        );
    }
}

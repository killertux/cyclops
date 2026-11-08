//! Settings: what each one means, its default, and how the places that set it combine.
//! See `specs/project/configuration.md`.

use crate::glob::Glob;
use crate::language::LanguageId;
use crate::model::TestCode;
use std::fmt;

/// A value as a settings file, the environment or a flag wrote it, before it is checked.
#[derive(Debug, Clone, PartialEq)]
pub enum RawValue {
    Bool(bool),
    Integer(i64),
    Text(String),
    List(Vec<RawValue>),
    /// Anything a setting can never be, such as a date or a table.
    Other,
}

/// Where a layer of settings comes from. Later places win.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    UserFile,
    ProjectFile,
    Environment,
    Flag,
}

/// The settings one place gives.
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    pub origin: Origin,
    /// Each setting by its full name, such as `tests.include`.
    pub values: Vec<(String, RawValue)>,
    /// Each alert rule, by the names of its fields.
    pub alerts: Vec<Vec<(String, RawValue)>>,
}

impl Layer {
    #[must_use]
    pub const fn new(origin: Origin) -> Self {
        Self {
            origin,
            values: Vec::new(),
            alerts: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsError {
    NotToml(String),
    Unreadable(String),
    Unknown(String),
    NotBool(&'static str),
    NotWholeNumber(&'static str),
    TooSmall(&'static str, i64),
    NotList(&'static str),
    NotOneOf(&'static str, &'static [&'static str]),
    UnknownLanguage(String),
    NotPeriod(&'static str),
    PersonalOnly(&'static str, String),
    AlwaysNotAllowed,
    /// An alert rule that can never work, with the reason.
    AlertField(String),
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotToml(file) => write!(f, "{file} is not valid TOML."),
            Self::Unreadable(file) => write!(f, "{file} cannot be read."),
            Self::Unknown(key) => write!(f, "Unknown setting: {key}"),
            Self::NotBool(key) => write!(f, "{key} must be true or false."),
            Self::NotWholeNumber(key) => write!(f, "{key} must be a whole number."),
            Self::TooSmall(key, min) => write!(f, "{key} must be at least {min}."),
            Self::NotList(key) => write!(f, "{key} must be a list."),
            Self::NotOneOf(key, choices) => {
                let (last, rest) = choices.split_last().unwrap_or((&"", &[]));
                write!(f, "{key} must be {} or {last}.", rest.join(", "))
            }
            Self::UnknownLanguage(name) => write!(
                f,
                "Unknown language: {name}. Supported: javascript, rust, typescript."
            ),
            Self::NotPeriod(key) => write!(
                f,
                "{key} must be a number of days, weeks, months or years, such as 12 months."
            ),
            Self::PersonalOnly(key, file) => write!(f, "{key} can only be set in {file}."),
            Self::AlertField(reason) => f.write_str(reason),
            Self::AlwaysNotAllowed => f.write_str(
                "semantic.mode can only be always in your own file, a flag or the environment.",
            ),
        }
    }
}

impl std::error::Error for SettingsError {}

/// When precise analysis runs. It runs when asked, unless told otherwise.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SemanticMode {
    Off,
    #[default]
    Manual,
    Always,
}

impl SemanticMode {
    /// The name settings use.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Manual => "manual",
            Self::Always => "always",
        }
    }
}

/// How serious an alert is. `cyclops check` fails on errors unless told otherwise.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Warning,
    #[default]
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Auto,
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeriodUnit {
    Days,
    Weeks,
    Months,
    Years,
}

/// A stretch of time back from now, such as `12 months`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Period {
    pub amount: u32,
    pub unit: PeriodUnit,
}

impl fmt::Display for Period {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let unit = match self.unit {
            PeriodUnit::Days => "day",
            PeriodUnit::Weeks => "week",
            PeriodUnit::Months => "month",
            PeriodUnit::Years => "year",
        };
        let plural = if self.amount == 1 { "" } else { "s" };
        write!(f, "{} {unit}{plural}", self.amount)
    }
}

/// An alert rule as a settings file wrote it. Rules are checked when they are used.
#[derive(Debug, Clone, PartialEq)]
pub struct AlertSource {
    pub fields: Vec<(String, RawValue)>,
    pub origin: Origin,
    /// Its place among the rules of its file, from 0.
    pub position: usize,
}

const ALERT_FIELDS: [&str; 7] = [
    "name", "metric", "scope", "when", "severity", "paths", "enabled",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingKey {
    ProjectLanguages,
    ProjectExclude,
    TestsInclude,
    TestsPaths,
    SemanticMode,
    DuplicationMinTokens,
    HotspotsSince,
    CheckFailOn,
    UiTheme,
}

/// What kind of value a setting holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingKind {
    Bool,
    WholeNumber,
    Text,
    List,
}

impl SettingKey {
    pub const ALL: [Self; 9] = [
        Self::ProjectLanguages,
        Self::ProjectExclude,
        Self::TestsInclude,
        Self::TestsPaths,
        Self::SemanticMode,
        Self::DuplicationMinTokens,
        Self::HotspotsSince,
        Self::CheckFailOn,
        Self::UiTheme,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::ProjectLanguages => "project.languages",
            Self::ProjectExclude => "project.exclude",
            Self::TestsInclude => "tests.include",
            Self::TestsPaths => "tests.paths",
            Self::SemanticMode => "semantic.mode",
            Self::DuplicationMinTokens => "metrics.duplication.min_tokens",
            Self::HotspotsSince => "metrics.hotspots.since",
            Self::CheckFailOn => "check.fail_on",
            Self::UiTheme => "ui.theme",
        }
    }

    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|key| key.name() == name)
    }

    #[must_use]
    pub const fn kind(self) -> SettingKind {
        match self {
            Self::ProjectLanguages | Self::ProjectExclude | Self::TestsPaths => SettingKind::List,
            Self::TestsInclude => SettingKind::Bool,
            Self::DuplicationMinTokens => SettingKind::WholeNumber,
            Self::SemanticMode | Self::HotspotsSince | Self::CheckFailOn | Self::UiTheme => {
                SettingKind::Text
            }
        }
    }

    /// Personal settings only make sense in your own file.
    #[must_use]
    pub const fn is_personal(self) -> bool {
        matches!(self, Self::UiTheme)
    }

    /// The environment variable that sets it, such as `CYCLOPS_TESTS_INCLUDE`.
    #[must_use]
    pub fn environment_variable(self) -> String {
        format!("CYCLOPS_{}", self.name().replace('.', "_").to_uppercase())
    }
}

/// Every setting, worked out.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub languages: Vec<LanguageId>,
    pub exclude: Vec<Glob>,
    pub tests: TestCode,
    pub test_paths: Vec<Glob>,
    pub semantic: SemanticMode,
    pub min_tokens: u32,
    pub hotspots_since: Period,
    pub fail_on: Severity,
    pub theme: Theme,
    pub alerts: Vec<AlertSource>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            languages: vec![
                LanguageId::JavaScript,
                LanguageId::Rust,
                LanguageId::TypeScript,
            ],
            exclude: Vec::new(),
            tests: TestCode::Excluded,
            test_paths: Vec::new(),
            semantic: SemanticMode::Manual,
            min_tokens: 50,
            hotspots_since: Period {
                amount: 12,
                unit: PeriodUnit::Months,
            },
            fail_on: Severity::Error,
            theme: Theme::Auto,
            alerts: Vec::new(),
        }
    }
}

fn language(name: &str) -> Option<LanguageId> {
    [
        LanguageId::JavaScript,
        LanguageId::Rust,
        LanguageId::TypeScript,
    ]
    .into_iter()
    .find(|language| language.name() == name)
}

fn choice<T: Copy>(
    key: &'static str,
    text: &str,
    choices: &'static [&'static str],
    values: &[T],
) -> Result<T, SettingsError> {
    choices
        .iter()
        .zip(values)
        .find(|(name, _)| **name == text)
        .map(|(_, value)| *value)
        .ok_or(SettingsError::NotOneOf(key, choices))
}

fn period(key: &'static str, text: &str) -> Result<Period, SettingsError> {
    let mut words = text.split_whitespace();
    let amount = words.next().and_then(|word| word.parse::<u32>().ok());
    let unit = match words.next() {
        Some("day" | "days") => Some(PeriodUnit::Days),
        Some("week" | "weeks") => Some(PeriodUnit::Weeks),
        Some("month" | "months") => Some(PeriodUnit::Months),
        Some("year" | "years") => Some(PeriodUnit::Years),
        _ => None,
    };
    match (amount, unit, words.next()) {
        (Some(amount), Some(unit), None) if amount > 0 => Ok(Period { amount, unit }),
        _ => Err(SettingsError::NotPeriod(key)),
    }
}

/// Reads a value written as text, the way the environment writes every value.
fn from_text(kind: SettingKind, text: &str) -> RawValue {
    match kind {
        SettingKind::Bool => match text {
            "true" => RawValue::Bool(true),
            "false" => RawValue::Bool(false),
            _ => RawValue::Text(text.to_owned()),
        },
        SettingKind::WholeNumber => text
            .parse()
            .map_or_else(|_| RawValue::Text(text.to_owned()), RawValue::Integer),
        SettingKind::Text => RawValue::Text(text.to_owned()),
        SettingKind::List => RawValue::List(
            text.split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(|item| RawValue::Text(item.to_owned()))
                .collect(),
        ),
    }
}

fn texts(key: &'static str, value: &RawValue) -> Result<Vec<String>, SettingsError> {
    let RawValue::List(items) = value else {
        return Err(SettingsError::NotList(key));
    };
    items
        .iter()
        .map(|item| match item {
            RawValue::Text(text) => Ok(text.clone()),
            _ => Err(SettingsError::NotList(key)),
        })
        .collect()
}

fn text(key: SettingKey, value: &RawValue) -> Result<&str, SettingsError> {
    match value {
        RawValue::Text(text) => Ok(text),
        _ => Err(match key {
            SettingKey::HotspotsSince => SettingsError::NotPeriod(key.name()),
            SettingKey::SemanticMode => SettingsError::NotOneOf(key.name(), SEMANTIC_MODES),
            SettingKey::CheckFailOn => SettingsError::NotOneOf(key.name(), SEVERITIES),
            _ => SettingsError::NotOneOf(key.name(), THEMES),
        }),
    }
}

const SEMANTIC_MODES: &[&str] = &["off", "manual", "always"];
const SEVERITIES: &[&str] = &["warning", "error"];
const THEMES: &[&str] = &["auto", "dark", "light"];

impl Settings {
    /// Combines the layers, from the one that matters least to the one that matters most.
    /// Every layer is checked in full, even where a later one wins, so that a mistake in a
    /// file never goes unnoticed. `user_file` is how messages name your own file.
    ///
    /// # Errors
    ///
    /// Fails on the first setting that is unknown, has the wrong kind of value, or is
    /// set somewhere it may not be.
    pub fn resolve(layers: &[Layer], user_file: &str) -> Result<Self, SettingsError> {
        let mut settings = Self::default();
        for layer in layers {
            for (name, value) in &layer.values {
                let key =
                    SettingKey::parse(name).ok_or_else(|| SettingsError::Unknown(name.clone()))?;
                if key.is_personal() && layer.origin != Origin::UserFile {
                    return Err(SettingsError::PersonalOnly(
                        key.name(),
                        user_file.to_owned(),
                    ));
                }
                settings.set(key, value, layer.origin)?;
            }
            for (position, fields) in layer.alerts.iter().enumerate() {
                if let Some((field, _)) = fields
                    .iter()
                    .find(|(field, _)| !ALERT_FIELDS.contains(&field.as_str()))
                {
                    return Err(SettingsError::Unknown(format!("alerts.{field}")));
                }
                settings.alerts.push(AlertSource {
                    fields: fields.clone(),
                    origin: layer.origin,
                    position,
                });
            }
        }
        Ok(settings)
    }

    /// The settings the environment gives, from its variables.
    pub fn environment_layer<'a>(variable: impl Fn(&str) -> Option<&'a str>) -> Layer {
        let mut layer = Layer::new(Origin::Environment);
        for key in SettingKey::ALL.into_iter().filter(|key| !key.is_personal()) {
            if let Some(value) = variable(&key.environment_variable()) {
                layer
                    .values
                    .push((key.name().to_owned(), from_text(key.kind(), value)));
            }
        }
        layer
    }

    fn set(
        &mut self,
        key: SettingKey,
        value: &RawValue,
        origin: Origin,
    ) -> Result<(), SettingsError> {
        let name = key.name();
        match key {
            SettingKey::ProjectLanguages => {
                let names = texts(name, value)?;
                let mut languages = names
                    .iter()
                    .map(|name| {
                        language(name).ok_or_else(|| SettingsError::UnknownLanguage(name.clone()))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                languages.sort_by_key(|language| language.name());
                languages.dedup();
                self.languages = languages;
            }
            SettingKey::ProjectExclude => {
                self.exclude = texts(name, value)?
                    .iter()
                    .map(|text| Glob::new(text))
                    .collect();
            }
            SettingKey::TestsPaths => {
                self.test_paths = texts(name, value)?
                    .iter()
                    .map(|text| Glob::new(text))
                    .collect();
            }
            SettingKey::TestsInclude => {
                let RawValue::Bool(include) = value else {
                    return Err(SettingsError::NotBool(name));
                };
                self.tests = if *include {
                    TestCode::Included
                } else {
                    TestCode::Excluded
                };
            }
            SettingKey::DuplicationMinTokens => {
                let RawValue::Integer(tokens) = value else {
                    return Err(SettingsError::NotWholeNumber(name));
                };
                self.min_tokens = u32::try_from(*tokens)
                    .ok()
                    .filter(|tokens| *tokens >= 1)
                    .ok_or(SettingsError::TooSmall(name, 1))?;
            }
            SettingKey::HotspotsSince => self.hotspots_since = period(name, text(key, value)?)?,
            SettingKey::SemanticMode => {
                let modes = [
                    SemanticMode::Off,
                    SemanticMode::Manual,
                    SemanticMode::Always,
                ];
                let mode = choice(name, text(key, value)?, SEMANTIC_MODES, &modes)?;
                if mode == SemanticMode::Always && origin == Origin::ProjectFile {
                    return Err(SettingsError::AlwaysNotAllowed);
                }
                self.semantic = mode;
            }
            SettingKey::CheckFailOn => {
                let severities = [Severity::Warning, Severity::Error];
                self.fail_on = choice(name, text(key, value)?, SEVERITIES, &severities)?;
            }
            SettingKey::UiTheme => {
                let themes = [Theme::Auto, Theme::Dark, Theme::Light];
                self.theme = choice(name, text(key, value)?, THEMES, &themes)?;
            }
        }
        Ok(())
    }

    /// A setting's value, the way every interface writes it.
    #[must_use]
    pub fn display(&self, key: SettingKey) -> String {
        fn list(items: impl Iterator<Item = String>) -> String {
            let items: Vec<String> = items.collect();
            if items.is_empty() {
                "none".to_owned()
            } else {
                items.join(", ")
            }
        }
        match key {
            SettingKey::ProjectLanguages => list(self.languages.iter().map(ToString::to_string)),
            SettingKey::ProjectExclude => list(self.exclude.iter().map(ToString::to_string)),
            SettingKey::TestsPaths => list(self.test_paths.iter().map(ToString::to_string)),
            SettingKey::TestsInclude => (self.tests == TestCode::Included).to_string(),
            SettingKey::SemanticMode => {
                let modes = [
                    SemanticMode::Off,
                    SemanticMode::Manual,
                    SemanticMode::Always,
                ];
                Self::named(SEMANTIC_MODES, &modes, &self.semantic)
            }
            SettingKey::DuplicationMinTokens => self.min_tokens.to_string(),
            SettingKey::HotspotsSince => self.hotspots_since.to_string(),
            SettingKey::CheckFailOn => Self::named(
                SEVERITIES,
                &[Severity::Warning, Severity::Error],
                &self.fail_on,
            ),
            SettingKey::UiTheme => Self::named(
                THEMES,
                &[Theme::Auto, Theme::Dark, Theme::Light],
                &self.theme,
            ),
        }
    }

    fn named<T: PartialEq>(names: &[&str], values: &[T], value: &T) -> String {
        names
            .iter()
            .zip(values)
            .find(|(_, candidate)| *candidate == value)
            .map_or_else(String::new, |(name, _)| (*name).to_owned())
    }
}

/// What `cyclops init` writes: the languages a project has, and a pointer to the rest.
#[must_use]
pub fn initial_config(languages: &[LanguageId]) -> String {
    let mut names: Vec<String> = languages
        .iter()
        .map(|language| format!("\"{language}\""))
        .collect();
    names.sort();
    format!(
        "# Cyclops settings for this project, shared with everyone who works on it.\n\
         # Every setting is optional. See specs/project/configuration.md in Cyclops.\n\
         \n\
         [project]\n\
         languages = [{}]\n",
        names.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const USER_FILE: &str = "~/.config/cyclops/config.toml";

    fn layer(origin: Origin, values: &[(&str, RawValue)]) -> Layer {
        Layer {
            origin,
            values: values
                .iter()
                .map(|(name, value)| ((*name).to_owned(), value.clone()))
                .collect(),
            alerts: Vec::new(),
        }
    }

    fn text(value: &str) -> RawValue {
        RawValue::Text(value.to_owned())
    }

    fn resolve(layers: &[Layer]) -> Result<Settings, String> {
        Settings::resolve(layers, USER_FILE).map_err(|error| error.to_string())
    }

    #[test]
    fn every_setting_has_a_default_and_can_be_written() {
        let settings = Settings::default();
        for key in SettingKey::ALL {
            assert!(!settings.display(key).is_empty(), "{}", key.name());
        }
    }

    #[test]
    fn later_layers_win() {
        let settings = resolve(&[
            layer(Origin::UserFile, &[("tests.include", RawValue::Bool(true))]),
            layer(
                Origin::ProjectFile,
                &[("tests.include", RawValue::Bool(false))],
            ),
        ])
        .expect("valid");
        assert_eq!(settings.tests, TestCode::Excluded);
    }

    #[test]
    fn a_mistake_is_caught_even_where_a_later_layer_wins() {
        let error = resolve(&[
            layer(Origin::ProjectFile, &[("tests.include", text("maybe"))]),
            layer(Origin::Flag, &[("tests.include", RawValue::Bool(true))]),
        ]);
        assert_eq!(
            error,
            Err("tests.include must be true or false.".to_owned())
        );
    }

    #[test]
    fn the_environment_writes_every_kind_of_value_as_text() {
        let variables = [
            ("CYCLOPS_TESTS_INCLUDE", "true"),
            ("CYCLOPS_PROJECT_LANGUAGES", "rust, typescript"),
            ("CYCLOPS_METRICS_DUPLICATION_MIN_TOKENS", "70"),
            ("CYCLOPS_UI_THEME", "light"),
        ];
        let layer = Settings::environment_layer(|name| {
            variables
                .iter()
                .find(|(variable, _)| *variable == name)
                .map(|(_, value)| *value)
        });
        let settings = resolve(&[layer]).expect("valid");
        assert_eq!(settings.tests, TestCode::Included);
        assert_eq!(
            settings.languages,
            [LanguageId::Rust, LanguageId::TypeScript]
        );
        assert_eq!(settings.min_tokens, 70);
        assert_eq!(
            settings.theme,
            Theme::Auto,
            "personal settings are not read from it"
        );
    }

    #[test]
    fn languages_are_listed_in_alphabetical_order() {
        let mixed = resolve(&[layer(
            Origin::ProjectFile,
            &[(
                "project.languages",
                RawValue::List(vec![text("typescript"), text("javascript")]),
            )],
        )])
        .expect("valid");
        assert_eq!(
            mixed.display(SettingKey::ProjectLanguages),
            "javascript, typescript"
        );
    }

    #[test]
    fn values_are_checked() {
        let cases = [
            (
                "semantic.mode",
                text("sometimes"),
                "semantic.mode must be off, manual or always.",
            ),
            (
                "check.fail_on",
                RawValue::Bool(true),
                "check.fail_on must be warning or error.",
            ),
            (
                "metrics.hotspots.since",
                text("soon"),
                "metrics.hotspots.since must be a number of days, weeks, months or years, such as 12 months.",
            ),
            (
                "metrics.duplication.min_tokens",
                text("many"),
                "metrics.duplication.min_tokens must be a whole number.",
            ),
            (
                "project.exclude",
                text("src"),
                "project.exclude must be a list.",
            ),
        ];
        for (name, value, message) in cases {
            let error = resolve(&[layer(Origin::ProjectFile, &[(name, value)])]);
            assert_eq!(error, Err(message.to_owned()), "{name}");
        }
    }

    #[test]
    fn a_period_can_be_written_in_several_units() {
        let settings = resolve(&[layer(
            Origin::ProjectFile,
            &[("metrics.hotspots.since", text("1 year"))],
        )])
        .expect("valid");
        assert_eq!(settings.hotspots_since.unit, PeriodUnit::Years);
        assert_eq!(settings.display(SettingKey::HotspotsSince), "1 year");
    }

    #[test]
    fn an_alert_rule_with_an_unknown_field_is_refused() {
        let mut project = Layer::new(Origin::ProjectFile);
        project
            .alerts
            .push(vec![("colour".to_owned(), text("red"))]);
        assert_eq!(
            resolve(&[project]),
            Err("Unknown setting: alerts.colour".to_owned())
        );
    }
}

//! Filters: the small expression language every interface uses to find symbols. See
//! `specs/explore/filters.md`.

use crate::glob::Glob;
use crate::language::LanguageId;
use crate::metric::MetricId;
use crate::symbol::{CodeRole, SymbolKind};
use std::fmt;

/// How deep parentheses and `not` may nest, so that no filter can exhaust the stack.
const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Comparison {
    Equal,
    NotEqual,
    Greater,
    GreaterOrEqual,
    Less,
    LessOrEqual,
}

impl Comparison {
    fn parse(text: &str) -> Option<Self> {
        match text {
            "=" => Some(Self::Equal),
            "!=" => Some(Self::NotEqual),
            ">" => Some(Self::Greater),
            ">=" => Some(Self::GreaterOrEqual),
            "<" => Some(Self::Less),
            "<=" => Some(Self::LessOrEqual),
            _ => None,
        }
    }

    /// How a filter writes it.
    #[must_use]
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Equal => "=",
            Self::NotEqual => "!=",
            Self::Greater => ">",
            Self::GreaterOrEqual => ">=",
            Self::Less => "<",
            Self::LessOrEqual => "<=",
        }
    }

    /// Whether `value` compares this way to `threshold`. Values are compared to two
    /// decimal places, the way they are written.
    #[must_use]
    pub fn holds(self, value: f64, threshold: f64) -> bool {
        let difference = value - threshold;
        let equal = difference.abs() < 0.005;
        match self {
            Self::Equal => equal,
            Self::NotEqual => !equal,
            Self::Greater => !equal && difference > 0.0,
            Self::GreaterOrEqual => equal || difference > 0.0,
            Self::Less => !equal && difference < 0.0,
            Self::LessOrEqual => equal || difference < 0.0,
        }
    }

    /// Reads a threshold such as `> 2`.
    #[must_use]
    pub fn parse_threshold(text: &str) -> Option<(Self, f64)> {
        let text = text.trim();
        let split = text
            .find(|character: char| !matches!(character, '<' | '>' | '=' | '!'))
            .unwrap_or(text.len());
        let (operator, value) = text.split_at_checked(split)?;
        Some((Self::parse(operator.trim())?, value.trim().parse().ok()?))
    }
}

/// One condition of a filter.
#[derive(Debug, Clone, PartialEq)]
pub enum Condition {
    Metric(MetricId, Comparison, f64),
    Kind(bool, SymbolKind),
    Role(bool, CodeRole),
    Language(bool, LanguageId),
    Path(PathTest),
    InCycle(bool),
}

#[derive(Debug, Clone, PartialEq)]
pub enum PathTest {
    Is(bool, String),
    Matches(Glob),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Filter {
    Test(Condition),
    Not(Box<Filter>),
    /// All of them. A long chain is one list, so it never nests deeply.
    And(Vec<Filter>),
    /// Any of them.
    Or(Vec<Filter>),
}

/// What a filter needs to know about a symbol.
pub trait Candidate {
    fn metric(&self, metric: MetricId) -> Option<f64>;
    fn kind(&self) -> SymbolKind;
    fn role(&self) -> CodeRole;
    fn language(&self) -> Option<LanguageId>;
    fn path(&self) -> Option<&str>;
    fn in_cycle(&self) -> Option<bool>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterError(String);

impl fmt::Display for FilterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for FilterError {}

fn invalid(reason: &str) -> FilterError {
    FilterError(format!("Invalid filter: {reason}"))
}

fn tokens(text: &str) -> Result<Vec<String>, FilterError> {
    let mut found = Vec::new();
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            c if c.is_whitespace() => {}
            '(' | ')' => found.push(character.to_string()),
            '"' => {
                let mut quoted = String::from('"');
                loop {
                    match characters.next() {
                        Some('"') => break,
                        Some(c) => quoted.push(c),
                        None => return Err(invalid("a quote is not closed")),
                    }
                }
                found.push(quoted);
            }
            '<' | '>' | '=' | '!' | '~' => {
                let mut operator = character.to_string();
                if characters.peek() == Some(&'=') && character != '=' && character != '~' {
                    operator.push('=');
                    characters.next();
                }
                found.push(operator);
            }
            _ => {
                let mut word = character.to_string();
                while let Some(&next) = characters.peek() {
                    if next.is_whitespace()
                        || matches!(next, '(' | ')' | '<' | '>' | '=' | '!' | '~' | '"')
                    {
                        break;
                    }
                    word.push(next);
                    characters.next();
                }
                found.push(word);
            }
        }
    }
    Ok(found)
}

/// Every kind a filter can name.
const KINDS: [SymbolKind; 11] = [
    SymbolKind::Package,
    SymbolKind::Module,
    SymbolKind::Struct,
    SymbolKind::Class,
    SymbolKind::Enum,
    SymbolKind::Interface,
    SymbolKind::Alias,
    SymbolKind::Function,
    SymbolKind::Method,
    SymbolKind::Field,
    SymbolKind::Constant,
];

/// The properties a filter can test, besides metrics, and what each one is.
const PROPERTIES: [(&str, &str); 5] = [
    ("kind", "What sort of symbol it is"),
    ("role", "Production or test code"),
    ("language", "The language it is written in"),
    ("path", "The file it is in"),
    ("in_cycle", "Whether its module is in a cycle"),
];

/// What can finish the last word of a filter being written, each with what it means:
/// a metric or a property, or a value after `property =`.
#[must_use]
pub fn completions(written: &str) -> Vec<(String, String)> {
    let words: Vec<&str> = written.split_whitespace().collect();
    let after_operator = words
        .last()
        .is_some_and(|last| Comparison::parse(last).is_some() || *last == "~");
    let ends_in_space = written.ends_with(char::is_whitespace) || after_operator;
    let (before, partial) = if ends_in_space {
        (words.as_slice(), "")
    } else {
        match words.split_last() {
            Some((last, before)) => (before, *last),
            None => (&[][..], ""),
        }
    };
    let values: Option<Vec<(String, String)>> = match before {
        [.., property, "=" | "!="] => match *property {
            "kind" => Some(
                KINDS
                    .iter()
                    .map(|kind| (kind.name().to_owned(), "a kind of symbol".to_owned()))
                    .collect(),
            ),
            "role" => Some(vec![
                ("production".to_owned(), "the product itself".to_owned()),
                ("test".to_owned(), "code that tests it".to_owned()),
            ]),
            "language" => Some(
                [
                    LanguageId::Rust,
                    LanguageId::TypeScript,
                    LanguageId::JavaScript,
                ]
                .iter()
                .map(|language| (language.name().to_owned(), "a language".to_owned()))
                .collect(),
            ),
            "in_cycle" => Some(vec![
                ("true".to_owned(), "in a cycle".to_owned()),
                ("false".to_owned(), "not in a cycle".to_owned()),
            ]),
            _ => None,
        },
        _ => None,
    };
    let candidates = values.unwrap_or_else(|| {
        let names = MetricId::ALL
            .iter()
            .map(|metric| (metric.name().to_owned(), metric.measures().to_owned()));
        let properties = PROPERTIES
            .iter()
            .map(|(name, meaning)| ((*name).to_owned(), (*meaning).to_owned()));
        let mut all: Vec<(String, String)> = names.chain(properties).collect();
        all.sort();
        all
    });
    if partial.is_empty() && !ends_in_space && !words.is_empty() {
        return Vec::new();
    }
    candidates
        .into_iter()
        .filter(|(name, _)| name.starts_with(partial))
        .collect()
}

fn kind(name: &str) -> Option<SymbolKind> {
    [
        SymbolKind::Package,
        SymbolKind::Module,
        SymbolKind::Struct,
        SymbolKind::Class,
        SymbolKind::Enum,
        SymbolKind::Interface,
        SymbolKind::Alias,
        SymbolKind::Function,
        SymbolKind::Method,
        SymbolKind::Field,
        SymbolKind::Constant,
    ]
    .into_iter()
    .find(|kind| kind.name() == name)
}

struct Parser {
    tokens: Vec<String>,
    position: usize,
    depth: usize,
}

impl Parser {
    fn peek(&self) -> Option<&str> {
        self.tokens.get(self.position).map(String::as_str)
    }

    fn next(&mut self) -> Option<String> {
        let token = self.tokens.get(self.position).cloned();
        self.position += 1;
        token
    }

    fn deeper(&mut self) -> Result<(), FilterError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            Err(invalid("it nests too deeply"))
        } else {
            Ok(())
        }
    }

    fn or(&mut self) -> Result<Filter, FilterError> {
        let mut parts = vec![self.and()?];
        while self.peek() == Some("or") {
            self.next();
            parts.push(self.and()?);
        }
        Ok(if parts.len() == 1 {
            parts.pop().ok_or_else(|| invalid("expected a condition"))?
        } else {
            Filter::Or(parts)
        })
    }

    fn and(&mut self) -> Result<Filter, FilterError> {
        let mut parts = vec![self.unary()?];
        while self.peek() == Some("and") {
            self.next();
            parts.push(self.unary()?);
        }
        Ok(if parts.len() == 1 {
            parts.pop().ok_or_else(|| invalid("expected a condition"))?
        } else {
            Filter::And(parts)
        })
    }

    fn unary(&mut self) -> Result<Filter, FilterError> {
        match self.peek() {
            Some("not") => {
                self.next();
                self.deeper()?;
                let inner = self.unary()?;
                self.depth -= 1;
                Ok(Filter::Not(Box::new(inner)))
            }
            Some("(") => {
                self.next();
                self.deeper()?;
                let inner = self.or()?;
                self.depth -= 1;
                match self.next().as_deref() {
                    Some(")") => Ok(inner),
                    _ => Err(invalid("expected )")),
                }
            }
            _ => self.condition().map(Filter::Test),
        }
    }

    fn condition(&mut self) -> Result<Condition, FilterError> {
        let field = self.next().ok_or_else(|| invalid("expected a condition"))?;
        let operator = self
            .next()
            .ok_or_else(|| invalid(&format!("expected a comparison after {field}")))?;
        let value = match self.next() {
            Some(value) if !matches!(value.as_str(), "and" | "or" | ")") => value,
            _ => return Err(invalid(&format!("expected a value after {operator}"))),
        };
        let unquoted = value.strip_prefix('"').unwrap_or(&value).to_owned();
        let equality = || match operator.as_str() {
            "=" => Ok(true),
            "!=" => Ok(false),
            other => Err(invalid(&format!("{field} cannot be compared with {other}"))),
        };
        match field.as_str() {
            "kind" => {
                let found = kind(&unquoted)
                    .ok_or_else(|| FilterError(format!("Unknown kind: {unquoted}")))?;
                Ok(Condition::Kind(equality()?, found))
            }
            "role" => {
                let role = match unquoted.as_str() {
                    "production" => CodeRole::Production,
                    "test" => CodeRole::Test,
                    other => {
                        return Err(FilterError(format!(
                            "Unknown role: {other}. Use production or test."
                        )));
                    }
                };
                Ok(Condition::Role(equality()?, role))
            }
            "language" => {
                let language = [
                    LanguageId::Rust,
                    LanguageId::TypeScript,
                    LanguageId::JavaScript,
                ]
                .into_iter()
                .find(|language| language.name() == unquoted)
                .ok_or_else(|| FilterError(format!("Unknown language: {unquoted}")))?;
                Ok(Condition::Language(equality()?, language))
            }
            "path" => match operator.as_str() {
                "~" => Ok(Condition::Path(PathTest::Matches(Glob::new(&unquoted)))),
                _ => Ok(Condition::Path(PathTest::Is(equality()?, unquoted))),
            },
            "in_cycle" => {
                let wanted = match unquoted.as_str() {
                    "true" => true,
                    "false" => false,
                    _ => return Err(invalid("in_cycle is true or false")),
                };
                Ok(Condition::InCycle(equality()? == wanted))
            }
            name => {
                let metric = MetricId::parse(name)
                    .ok_or_else(|| FilterError(format!("Unknown metric or property: {name}")))?;
                let comparison = Comparison::parse(&operator).ok_or_else(|| {
                    invalid(&format!("{name} cannot be compared with {operator}"))
                })?;
                let number = unquoted
                    .parse()
                    .map_err(|_| invalid(&format!("{unquoted} is not a number")))?;
                Ok(Condition::Metric(metric, comparison, number))
            }
        }
    }
}

impl Filter {
    /// Reads a filter.
    ///
    /// # Errors
    ///
    /// Fails, and says why, for anything it cannot understand.
    pub fn parse(text: &str) -> Result<Self, FilterError> {
        let mut parser = Parser {
            tokens: tokens(text)?,
            position: 0,
            depth: 0,
        };
        let filter = parser.or()?;
        match parser.peek() {
            None => Ok(filter),
            Some(rest) => Err(invalid(&format!("unexpected {rest}"))),
        }
    }

    /// Whether a symbol matches. A symbol without a value for a metric never matches a
    /// condition on it. The work stays on its own stack, so no filter can overflow.
    pub fn matches(&self, candidate: &dyn Candidate) -> bool {
        enum Step<'a> {
            Visit(&'a Filter),
            Not,
            And(usize),
            Or(usize),
        }
        let mut steps = vec![Step::Visit(self)];
        let mut values: Vec<bool> = Vec::new();
        while let Some(step) = steps.pop() {
            match step {
                Step::Visit(Filter::Test(condition)) => values.push(test(condition, candidate)),
                Step::Visit(Filter::Not(inner)) => {
                    steps.push(Step::Not);
                    steps.push(Step::Visit(inner));
                }
                Step::Visit(Filter::And(parts)) => {
                    steps.push(Step::And(parts.len()));
                    steps.extend(parts.iter().rev().map(Step::Visit));
                }
                Step::Visit(Filter::Or(parts)) => {
                    steps.push(Step::Or(parts.len()));
                    steps.extend(parts.iter().rev().map(Step::Visit));
                }
                Step::Not => {
                    let value = values.pop().unwrap_or(false);
                    values.push(!value);
                }
                Step::And(count) | Step::Or(count) => {
                    let parts = values.split_off(values.len().saturating_sub(count));
                    values.push(if matches!(step, Step::And(_)) {
                        parts.iter().all(|value| *value)
                    } else {
                        parts.iter().any(|value| *value)
                    });
                }
            }
        }
        values.pop().unwrap_or(false)
    }
}

fn test(condition: &Condition, candidate: &dyn Candidate) -> bool {
    match condition {
        Condition::Metric(metric, comparison, threshold) => candidate
            .metric(*metric)
            .is_some_and(|value| comparison.holds(value, *threshold)),
        Condition::Kind(equal, wanted) => {
            let kind = candidate.kind();
            let is =
                kind == *wanted || (*wanted == SymbolKind::Function && kind == SymbolKind::Method);
            is == *equal
        }
        Condition::Role(equal, role) => (candidate.role() == *role) == *equal,
        Condition::Language(equal, language) => candidate
            .language()
            .is_some_and(|found| (found == *language) == *equal),
        Condition::Path(PathTest::Matches(glob)) => {
            candidate.path().is_some_and(|path| glob.matches(path))
        }
        Condition::Path(PathTest::Is(equal, wanted)) => candidate
            .path()
            .is_some_and(|path| (path == wanted) == *equal),
        Condition::InCycle(wanted) => candidate.in_cycle() == Some(*wanted),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Symbol {
        kind: SymbolKind,
        cyclomatic: Option<f64>,
        path: &'static str,
    }

    impl Candidate for Symbol {
        fn metric(&self, metric: MetricId) -> Option<f64> {
            (metric == MetricId::Cyclomatic)
                .then_some(self.cyclomatic)
                .flatten()
        }
        fn kind(&self) -> SymbolKind {
            self.kind
        }
        fn role(&self) -> CodeRole {
            CodeRole::Production
        }
        fn language(&self) -> Option<LanguageId> {
            Some(LanguageId::Rust)
        }
        fn path(&self) -> Option<&str> {
            Some(self.path)
        }
        fn in_cycle(&self) -> Option<bool> {
            None
        }
    }

    const METHOD: Symbol = Symbol {
        kind: SymbolKind::Method,
        cyclomatic: Some(3.0),
        path: "src/orders.rs",
    };

    fn matches(filter: &str) -> bool {
        Filter::parse(filter).expect("valid").matches(&METHOD)
    }

    fn error(filter: &str) -> String {
        Filter::parse(filter).expect_err("invalid").to_string()
    }

    #[test]
    fn not_binds_tighter_than_and_which_binds_tighter_than_or() {
        assert!(matches("kind = struct or kind = method and cyclomatic > 2"));
        assert!(!matches(
            "(kind = struct or kind = method) and cyclomatic > 3"
        ));
        assert!(matches("not kind = struct and cyclomatic >= 3"));
    }

    #[test]
    fn a_function_filter_finds_methods_too() {
        assert!(matches("kind = function"));
        assert!(matches("kind = method"));
    }

    #[test]
    fn a_path_can_be_a_pattern() {
        assert!(matches("path ~ \"src/o*\""));
        assert!(!matches("path ~ \"tests/**\""));
        assert!(matches("path = src/orders.rs"));
    }

    #[test]
    fn mistakes_are_explained() {
        assert_eq!(
            error("nonsense > 1"),
            "Unknown metric or property: nonsense"
        );
        assert_eq!(
            error("cyclomatic >"),
            "Invalid filter: expected a value after >"
        );
        assert_eq!(error("kind = widget"), "Unknown kind: widget");
    }

    #[test]
    fn completions_finish_a_metric_a_property_or_a_value() {
        let names = |written: &str| -> Vec<String> {
            completions(written)
                .into_iter()
                .map(|(name, _)| name)
                .collect()
        };
        assert_eq!(names("cog"), vec!["cognitive"]);
        assert_eq!(
            names("kind = struct and cy"),
            vec!["cycle_size", "cyclomatic"]
        );
        assert_eq!(names("role = "), vec!["production", "test"]);
        assert_eq!(names("role ="), vec!["production", "test"]);
        assert_eq!(names("kind = st"), vec!["struct"]);
        assert_eq!(
            completions("cognitive")
                .first()
                .map(|(_, meaning)| meaning.as_str()),
            Some("Difficulty to follow")
        );
        assert_eq!(error("(kind = struct"), "Invalid filter: expected )");
        assert_eq!(
            error("cyclomatic > 1 kind"),
            "Invalid filter: unexpected kind"
        );
    }

    #[test]
    fn deep_nesting_is_refused_rather_than_overflowing() {
        let deep = format!("{}cyclomatic > 1{}", "(".repeat(10_000), ")".repeat(10_000));
        assert_eq!(error(&deep), "Invalid filter: it nests too deeply");
        let long = vec!["cyclomatic > 1"; 50_000].join(" and ");
        assert!(matches(&long));
    }

    #[test]
    fn thresholds_read_their_comparison() {
        assert_eq!(
            Comparison::parse_threshold("> 2"),
            Some((Comparison::Greater, 2.0))
        );
        assert_eq!(
            Comparison::parse_threshold(">=1"),
            Some((Comparison::GreaterOrEqual, 1.0))
        );
        assert_eq!(Comparison::parse_threshold("around 1"), None);
    }
}

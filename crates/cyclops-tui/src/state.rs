//! The words of the interface: views, modes, panes, popups, and what it asks the world
//! to do.

use cyclops_domain::metric::MetricId;
use cyclops_domain::symbol::{SymbolId, SymbolKind};

/// One of the screens. One is showing at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum View {
    Overview,
    Map,
    Explore,
    Diagram,
    Find,
    Alerts,
    Status,
}

impl View {
    /// In the order of the tab bar.
    pub const ALL: [Self; 7] = [
        Self::Overview,
        Self::Map,
        Self::Explore,
        Self::Diagram,
        Self::Find,
        Self::Alerts,
        Self::Status,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Map => "Map",
            Self::Explore => "Explore",
            Self::Diagram => "Diagram",
            Self::Find => "Find",
            Self::Alerts => "Alerts",
            Self::Status => "Status",
        }
    }

    /// The key that goes to it after `Space`.
    #[must_use]
    pub const fn letter(self) -> char {
        match self {
            Self::Overview => 'o',
            Self::Map => 'm',
            Self::Explore => 'e',
            Self::Diagram => 'd',
            Self::Find => 'f',
            Self::Alerts => 'a',
            Self::Status => 's',
        }
    }

    #[must_use]
    pub fn by_letter(letter: char) -> Option<Self> {
        Self::ALL.into_iter().find(|view| view.letter() == letter)
    }

    #[must_use]
    pub fn position(self) -> usize {
        Self::ALL
            .iter()
            .position(|view| *view == self)
            .unwrap_or_default()
    }

    /// Whether the view can be about a symbol of this kind. One that cannot sends a jump
    /// to Explore.
    #[must_use]
    pub const fn shows(self, kind: SymbolKind) -> bool {
        match self {
            Self::Explore | Self::Map => true,
            Self::Diagram => !matches!(
                kind,
                SymbolKind::Function
                    | SymbolKind::Method
                    | SymbolKind::Field
                    | SymbolKind::Constant
            ),
            Self::Overview | Self::Find | Self::Alerts | Self::Status => false,
        }
    }
}

/// What keys mean now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Command,
    Filter,
    Search,
}

impl Mode {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Command => "COMMAND",
            Self::Filter => "FILTER",
            Self::Search => "SEARCH",
        }
    }
}

/// A section of the inspector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Metrics,
    Why,
    Relations,
    Clones,
    Impact,
}

impl Section {
    pub const ALL: [Self; 5] = [
        Self::Metrics,
        Self::Why,
        Self::Relations,
        Self::Clones,
        Self::Impact,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Metrics => "Metrics",
            Self::Why => "Why",
            Self::Relations => "Relations",
            Self::Clones => "Clones",
            Self::Impact => "Impact",
        }
    }

    /// The next one, or the previous one, wrapping around.
    #[must_use]
    pub fn step(self, forward: bool) -> Self {
        let count = Self::ALL.len();
        let here = Self::ALL
            .iter()
            .position(|section| *section == self)
            .unwrap_or_default();
        let next = if forward {
            (here + 1) % count
        } else {
            (here + count - 1) % count
        };
        Self::ALL.get(next).copied().unwrap_or(self)
    }
}

/// A pane that can have the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Metrics,
    Ranking,
    Structure,
    Diagram,
    Map,
    Results,
    Rules,
    Hits,
    Status,
    Inspector,
}

impl Pane {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Metrics => "metrics",
            Self::Ranking => "ranking",
            Self::Structure => "structure",
            Self::Diagram => "diagram",
            Self::Map => "map",
            Self::Results => "results",
            Self::Rules => "rules",
            Self::Hits => "hits",
            Self::Status => "status",
            Self::Inspector => "inspector",
        }
    }
}

/// A one-line prompt that replaces the hint line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    Command,
    Filter,
    Search,
    Jump,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub kind: PromptKind,
    pub text: String,
    /// Why what was typed was refused, shown beside it.
    pub error: Option<String>,
}

/// A popup over the view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Popup {
    SpaceMenu,
    MetricPicker {
        typed: String,
    },
    /// The keys of the view, or of everything.
    Help {
        all: bool,
    },
}

impl Popup {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::SpaceMenu => "space menu",
            Self::MetricPicker { .. } => "metric picker",
            Self::Help { .. } => "help",
        }
    }
}

/// What the interface asks the world to do. Whoever runs it does the work, and tells it
/// the result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Analyze again. `tests` overrides the setting for this session.
    Analyze {
        tests: Option<bool>,
        precise: bool,
    },
    /// Save default settings for the team.
    Init,
    Quit,
}

/// A setting of the theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeChoice {
    Auto,
    Dark,
    Light,
}

impl ThemeChoice {
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "auto" => Some(Self::Auto),
            "dark" => Some(Self::Dark),
            "light" => Some(Self::Light),
            _ => None,
        }
    }
}

/// The colors the interface draws with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Dark,
    Light,
    NoColor,
}

impl Theme {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
            Self::NoColor => "no color",
        }
    }
}

/// How the interface starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Startup {
    pub columns: u16,
    pub rows: u16,
    /// `ui.theme` from the settings.
    pub theme: ThemeChoice,
    /// What the terminal said its background is, if it answered.
    pub background: Option<ThemeChoice>,
    /// `NO_COLOR` is set.
    pub no_color: bool,
}

/// Something on screen the mouse can point at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Tab(View),
    Symbol(SymbolId),
    /// An entry of the Space menu, by its key.
    Menu(char),
}

/// A place on the trail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub view: View,
    pub focus: Option<SymbolId>,
    pub section: Option<Section>,
}

/// The main metrics, in the order of the Overview list.
pub const OVERVIEW_METRICS: [MetricId; 14] = [
    MetricId::Cyclomatic,
    MetricId::Cognitive,
    MetricId::MaxNesting,
    MetricId::Lines,
    MetricId::Parameters,
    MetricId::Duplication,
    MetricId::Lcom,
    MetricId::Afferent,
    MetricId::Efferent,
    MetricId::Instability,
    MetricId::Distance,
    MetricId::CycleSize,
    MetricId::Churn,
    MetricId::Hotspot,
];

/// How the interface names a metric.
#[must_use]
pub const fn metric_label(metric: MetricId) -> &'static str {
    match metric {
        MetricId::CycleSize => "cycle size",
        other => other.name(),
    }
}

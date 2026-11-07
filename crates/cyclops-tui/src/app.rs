//! The interface as a model and an update: what it holds, and how each input changes
//! it. Nothing here touches a terminal, so it can be driven by a test as well as a person.

use crate::input::{Input, Key, MouseKind};
use crate::state::{
    Effect, Mode, OVERVIEW_METRICS, Pane, Place, Popup, Pose, Prompt, PromptKind, Section, Startup,
    Target, Theme, ThemeChoice, View, metric_label,
};
use cyclops_application::analysis::ProjectAnalysis;
use cyclops_application::diagram::{DiagramRequest, DiagramView, Level};
use cyclops_application::ports::{Partial, Step};
use cyclops_application::queries::Order;
use cyclops_domain::filter::Filter;
use cyclops_domain::filter::completions;
use cyclops_domain::metric::{MetricId, Subject};
use cyclops_domain::model::TestCode;
use cyclops_domain::problem::Problem;
use cyclops_domain::relation::{Confidence, RelationKind};
use cyclops_domain::settings::Severity;
use cyclops_domain::symbol::{Symbol, SymbolId, SymbolKind};
use ratatui::layout::Rect;
use std::collections::{BTreeMap, HashMap, HashSet};

/// The smallest terminal the interface works in.
pub const MIN_COLUMNS: u16 = 80;
pub const MIN_ROWS: u16 = 24;
/// Below this width an open inspector covers the view instead of sitting beside it.
pub const SIDE_BY_SIDE: u16 = 100;
/// How long a prefix waits for its second key before listing what can follow.
pub const HINT_DELAY_MS: u64 = 500;
/// How long the mascot shows it found new alerts.
pub const FOUND_MS: u64 = 3000;

/// The commands of the command line, for completion.
const COMMANDS: [&str; 7] = ["q", "w", "w user", "init", "export", "set bg=", "help"];

/// The filter of the lens: what was typed, and what it means.
#[derive(Debug, Clone)]
pub struct LensFilter {
    pub text: String,
    pub filter: Filter,
}

/// The focus, the metric, the filter and the test code setting.
#[derive(Debug, Clone)]
pub struct Lens {
    pub metric: MetricId,
    /// Whether the person picked the metric, so that a new analysis keeps it.
    pub chosen: bool,
    pub filter: Option<LensFilter>,
    pub tests: bool,
}

/// One row of the Explore tree.
#[derive(Debug, Clone, Copy)]
pub struct TreeRow<'a> {
    pub symbol: &'a Symbol,
    pub depth: usize,
    /// Whether it is open, or nothing when there is nothing inside it.
    pub open: Option<bool>,
}

/// A node of the diagram, and where it sits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    pub id: SymbolId,
    pub column: usize,
    pub row: usize,
}

/// The diagram the view shows.
#[derive(Debug, Clone)]
pub struct Drawing {
    pub view: DiagramView,
    pub level: Level,
    pub depth: u32,
    pub placed: Vec<Placed>,
}

/// A drill into the diagram, to go back up from.
#[derive(Debug, Clone)]
struct Drill {
    focus: Option<SymbolId>,
    depth: u32,
}

#[derive(Debug, Clone, Default)]
struct DiagramState {
    selected: Option<SymbolId>,
    depth: Option<u32>,
    level: Option<Level>,
    drills: Vec<Drill>,
}

/// What the interface hears from an analysis while it runs.
#[derive(Debug, Clone, PartialEq)]
pub enum Heard {
    /// A step is starting.
    Step(Step),
    /// Files read so far, of how many.
    Files(usize, usize),
    /// What the files read so far say.
    Partial(Partial),
    /// The indexer that is about to run.
    Indexer(String),
}

/// An analysis that is running.
#[derive(Debug, Clone, Default)]
pub struct Running {
    /// The step it is on, once it has started one.
    pub step: Option<Step>,
    /// Files read, of how many.
    pub files: (usize, usize),
    pub partial: Option<Partial>,
    /// The indexer running, in a precise analysis.
    pub indexer: Option<String>,
    /// When it started, in milliseconds since the interface opened.
    pub since: u64,
    pub precise: bool,
}

impl Running {
    /// Whether a step has finished.
    #[must_use]
    pub fn past(&self, step: Step) -> bool {
        let order = |step: Step| Step::PIPELINE.iter().position(|one| *one == step);
        match (self.step.and_then(order), order(step)) {
            (Some(now), Some(asked)) => now > asked,
            _ => false,
        }
    }
}

/// The colors to draw with, and where each choice came from.
#[derive(Debug, Clone, Copy)]
struct Colors {
    setting: ThemeChoice,
    session: Option<ThemeChoice>,
    background: Option<ThemeChoice>,
    no_color: bool,
}

/// What the interface knows about the project and the person's setup.
#[derive(Debug, Clone, Copy)]
struct Project {
    /// The project has settings of its own.
    has_settings: bool,
    /// There is an editor to open files in.
    editor: bool,
}

/// What Find remembers.
#[derive(Debug, Clone, Default)]
struct FindState {
    index: usize,
    /// The metric it sorts by, and whether lowest first, once a column was picked.
    sort: Option<(MetricId, bool)>,
    /// The filters used in this session, oldest first.
    history: Vec<String>,
    /// Where `Up` and `Down` are in the history.
    recalled: Option<usize>,
    /// The last filter typed that could be understood, while one is typed.
    live: Option<Filter>,
}

/// The trail of jumps, like Vim's jump list.
#[derive(Debug, Clone)]
struct Trail {
    places: Vec<Place>,
    position: usize,
}

/// The interface.
pub struct App {
    columns: u16,
    rows: u16,
    running: bool,
    analysis: Option<ProjectAnalysis>,
    /// Why the analysis was refused, if it was.
    failure: Option<String>,
    progress: Option<Running>,
    stale: bool,
    view: View,
    prompt: Option<Prompt>,
    popup: Option<Popup>,
    lens: Lens,
    tests_override: Option<bool>,
    focus: Option<SymbolId>,
    expanded: HashSet<SymbolId>,
    diagram: DiagramState,
    map_selected: Option<SymbolId>,
    find: FindState,
    ranking_index: usize,
    /// The row of the inspector's list that is selected.
    inspector_row: usize,
    pane: usize,
    inspector: Option<Section>,
    trail: Trail,
    marks: HashMap<char, SymbolId>,
    count: Option<u32>,
    /// A prefix key waiting for the next one, and when it was pressed.
    pending: Option<(Key, u64)>,
    now: u64,
    message: Option<String>,
    search: Option<String>,
    colors: Colors,
    project: Project,
    /// What was copied last.
    copied: Option<String>,
    /// What was last sent to the editor, as `PATH:LINE`.
    opened: Option<String>,
    /// When a refresh found new alerts.
    found_alerts: Option<u64>,
    /// How many analyses have finished.
    analyses: usize,
    hits: Vec<(Rect, Target)>,
    effects: Vec<Effect>,
}

fn parent_of(symbol: &Symbol) -> Option<&SymbolId> {
    symbol.parent.as_ref()
}

impl App {
    /// The interface as it opens, waiting for its first analysis.
    #[must_use]
    pub fn new(startup: Startup) -> Self {
        let start = Place {
            view: View::Overview,
            focus: None,
            section: None,
        };
        Self {
            columns: startup.columns,
            rows: startup.rows,
            running: true,
            analysis: None,
            failure: None,
            progress: Some(Running::default()),
            stale: false,
            view: View::Overview,
            prompt: None,
            popup: None,
            lens: Lens {
                metric: MetricId::Cyclomatic,
                chosen: false,
                filter: None,
                tests: false,
            },
            tests_override: None,
            focus: None,
            expanded: HashSet::new(),
            diagram: DiagramState::default(),
            map_selected: None,
            find: FindState::default(),
            ranking_index: 0,
            inspector_row: 0,
            pane: 0,
            inspector: None,
            trail: Trail {
                places: vec![start],
                position: 0,
            },
            marks: HashMap::new(),
            count: None,
            pending: None,
            now: 0,
            message: None,
            search: None,
            colors: Colors {
                setting: startup.theme,
                session: None,
                background: startup.background,
                no_color: startup.no_color,
            },
            project: Project {
                has_settings: startup.initialized,
                editor: startup.editor,
            },
            copied: None,
            opened: None,
            found_alerts: None,
            analyses: 0,
            hits: Vec::new(),
            effects: vec![Effect::Analyze {
                tests: None,
                precise: false,
            }],
        }
    }

    // What the interface shows.

    #[must_use]
    pub const fn running(&self) -> bool {
        self.running
    }

    #[must_use]
    pub const fn size(&self) -> (u16, u16) {
        (self.columns, self.rows)
    }

    #[must_use]
    pub const fn too_small(&self) -> bool {
        self.columns < MIN_COLUMNS || self.rows < MIN_ROWS
    }

    #[must_use]
    pub const fn analysis(&self) -> Option<&ProjectAnalysis> {
        self.analysis.as_ref()
    }

    #[must_use]
    pub fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }

    #[must_use]
    pub const fn analyzing(&self) -> bool {
        self.progress.is_some()
    }

    /// The analysis that is running, if one is.
    #[must_use]
    pub const fn running_analysis(&self) -> Option<&Running> {
        self.progress.as_ref()
    }

    #[must_use]
    pub const fn stale(&self) -> bool {
        self.stale
    }

    /// Whether the project has settings of its own.
    #[must_use]
    pub const fn has_settings(&self) -> bool {
        self.project.has_settings
    }

    /// What was copied last.
    #[must_use]
    pub fn copied(&self) -> Option<&str> {
        self.copied.as_deref()
    }

    /// What was last sent to the editor, as `PATH:LINE`.
    #[must_use]
    pub fn opened(&self) -> Option<&str> {
        self.opened.as_deref()
    }

    /// What the mascot is doing.
    #[must_use]
    pub fn pose(&self) -> Pose {
        if self.progress.is_some() {
            return Pose::Scanning;
        }
        if self
            .found_alerts
            .is_some_and(|since| self.now < since + FOUND_MS)
        {
            return Pose::Found;
        }
        let nothing = self
            .analysis
            .as_ref()
            .is_some_and(|analysis| analysis.problems().contains(&Problem::NoSourceFiles));
        if nothing {
            Pose::NothingHere
        } else {
            Pose::Watching
        }
    }

    /// The time, in milliseconds since the interface opened.
    #[must_use]
    pub const fn now(&self) -> u64 {
        self.now
    }

    /// The selected row of the Overview ranking.
    #[must_use]
    pub const fn ranking_row(&self) -> usize {
        self.ranking_index
    }

    /// How many files the last refresh parsed, or 0 before any refresh.
    #[must_use]
    pub fn parsed_by_refresh(&self) -> usize {
        if self.analyses > 1 {
            self.analysis
                .as_ref()
                .map_or(0, ProjectAnalysis::files_parsed)
        } else {
            0
        }
    }

    /// The row of the inspector's list that is selected.
    #[must_use]
    pub const fn inspector_row(&self) -> usize {
        self.inspector_row
    }

    /// The order Find lists in: a metric, and whether lowest first.
    #[must_use]
    pub fn sorted_by(&self) -> (MetricId, bool) {
        self.find.sort.unwrap_or((self.lens.metric, false))
    }

    #[must_use]
    pub const fn view(&self) -> View {
        self.view
    }

    #[must_use]
    pub fn mode(&self) -> Mode {
        match (&self.prompt, &self.popup) {
            (Some(prompt), _) => match prompt.kind {
                PromptKind::Command => Mode::Command,
                PromptKind::Filter => Mode::Filter,
                PromptKind::Search | PromptKind::Jump => Mode::Search,
            },
            (None, Some(Popup::MetricPicker { .. } | Popup::SortPicker { .. })) => Mode::Search,
            (None, Some(Popup::ConfirmPrecise)) => Mode::Confirm,
            (None, _) if self.progress.is_some() => Mode::Analyzing,
            (None, _) => Mode::Normal,
        }
    }

    #[must_use]
    pub const fn prompt(&self) -> Option<&Prompt> {
        self.prompt.as_ref()
    }

    #[must_use]
    pub const fn popup(&self) -> Option<&Popup> {
        self.popup.as_ref()
    }

    /// The prefix whose hints are showing, once it has waited long enough.
    #[must_use]
    pub fn hinting(&self) -> Option<char> {
        let (key, since) = self.pending?;
        let Key::Char(prefix) = key else {
            return None;
        };
        (matches!(prefix, 'g' | 'z' | ']' | '[') && self.now >= since + HINT_DELAY_MS)
            .then_some(prefix)
    }

    /// The name of the popup that is showing, if any.
    #[must_use]
    pub fn popup_name(&self) -> Option<&'static str> {
        if let Some(popup) = &self.popup {
            return Some(popup.name());
        }
        self.hinting().map(|_| "key hints")
    }

    /// The keys the open popup lists, in order.
    #[must_use]
    pub fn popup_keys(&self) -> Vec<char> {
        let mut keys = match (&self.popup, self.hinting()) {
            (Some(Popup::SpaceMenu), _) => space_menu()
                .iter()
                .flat_map(|(_, entries)| entries.iter().map(|(key, _)| *key))
                .collect(),
            (None, Some(prefix)) => hints(prefix).iter().map(|(key, _)| *key).collect(),
            _ => Vec::new(),
        };
        keys.sort_unstable();
        keys
    }

    #[must_use]
    pub const fn lens(&self) -> &Lens {
        &self.lens
    }

    #[must_use]
    pub const fn focus(&self) -> Option<&SymbolId> {
        self.focus.as_ref()
    }

    #[must_use]
    pub const fn inspector(&self) -> Option<Section> {
        self.inspector
    }

    #[must_use]
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// The place on the trail, and how long it is.
    #[must_use]
    pub const fn trail(&self) -> (usize, usize) {
        (self.trail.position + 1, self.trail.places.len())
    }

    /// The marks set on a symbol, in order.
    #[must_use]
    pub fn marks_of(&self, id: &SymbolId) -> Vec<char> {
        let mut marks: Vec<char> = self
            .marks
            .iter()
            .filter(|(_, marked)| *marked == id)
            .map(|(mark, _)| *mark)
            .collect();
        marks.sort_unstable();
        marks
    }

    #[must_use]
    pub fn theme(&self) -> Theme {
        if self.colors.no_color {
            return Theme::NoColor;
        }
        match self.colors.session.unwrap_or(self.colors.setting) {
            ThemeChoice::Dark => Theme::Dark,
            ThemeChoice::Light => Theme::Light,
            ThemeChoice::Auto => match self.colors.background {
                Some(ThemeChoice::Light) => Theme::Light,
                _ => Theme::Dark,
            },
        }
    }

    /// Whether a precise analysis has finished, so that relations may be exact.
    #[must_use]
    pub fn precise_layer(&self) -> bool {
        self.analysis
            .as_ref()
            .is_some_and(|analysis| analysis.precise().current.is_some())
    }

    /// Whether the interface is waiting for the world to do something.
    #[must_use]
    pub fn has_effects(&self) -> bool {
        !self.effects.is_empty()
    }

    /// What the world needs to do for the interface, which it then reports back.
    pub fn take_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.effects)
    }

    /// Where on screen each thing the mouse can point at was drawn.
    pub fn set_hits(&mut self, hits: Vec<(Rect, Target)>) {
        self.hits = hits;
    }

    #[must_use]
    pub fn hits(&self) -> &[(Rect, Target)] {
        &self.hits
    }

    #[must_use]
    pub fn symbol(&self, id: &SymbolId) -> Option<&Symbol> {
        self.analysis.as_ref()?.view().symbol(id)
    }

    fn name_of(&self, id: &SymbolId) -> String {
        self.symbol(id)
            .map_or_else(|| id.to_string(), |symbol| symbol.name.clone())
    }

    /// The parts of the breadcrumb, from the top down to the focus.
    #[must_use]
    pub fn breadcrumb(&self) -> Vec<String> {
        let mut parts = Vec::new();
        let mut current = self.focus.as_ref().and_then(|id| self.symbol(id));
        while let Some(symbol) = current {
            parts.push(symbol.name.clone());
            current = parent_of(symbol).and_then(|parent| self.symbol(parent));
        }
        parts.reverse();
        parts
    }

    /// The panes showing, in order.
    #[must_use]
    pub fn panes(&self) -> Vec<Pane> {
        if self.too_small() {
            return Vec::new();
        }
        let main: &[Pane] = match self.view {
            View::Overview => &[Pane::Metrics, Pane::Ranking],
            View::Map => &[Pane::Map],
            View::Explore => &[Pane::Structure],
            View::Diagram => &[Pane::Diagram],
            View::Find => &[Pane::Results],
            View::Alerts => &[Pane::Rules, Pane::Hits],
            View::Status => &[Pane::Status],
        };
        if self.inspector.is_none() {
            return main.to_vec();
        }
        if self.columns < SIDE_BY_SIDE {
            return vec![Pane::Inspector];
        }
        let mut panes = main.to_vec();
        panes.push(Pane::Inspector);
        panes
    }

    /// The pane that has the keyboard.
    #[must_use]
    pub fn pane(&self) -> Option<Pane> {
        let panes = self.panes();
        panes
            .get(self.pane.min(panes.len().saturating_sub(1)))
            .copied()
    }

    fn kept(&self) -> Option<HashSet<SymbolId>> {
        let analysis = self.analysis.as_ref()?;
        let filter = self.lens.filter.as_ref()?;
        let view = analysis.view();
        let mut kept = HashSet::new();
        for found in analysis.search(&filter.filter, None, None) {
            let mut current = Some(found);
            while let Some(symbol) = current {
                if !kept.insert(symbol.id.clone()) {
                    break;
                }
                current = parent_of(symbol).and_then(|parent| view.symbol(parent));
            }
        }
        Some(kept)
    }

    /// The rows of the Explore tree, in order.
    #[must_use]
    pub fn tree(&self) -> Vec<TreeRow<'_>> {
        let Some(analysis) = &self.analysis else {
            return Vec::new();
        };
        let kept = self.kept();
        let shown = |symbol: &&Symbol| kept.as_ref().is_none_or(|kept| kept.contains(&symbol.id));
        let metric = self.lens.metric;
        let mut rows = Vec::new();
        let mut pending: Vec<(&Symbol, usize)> = analysis
            .roots_by(metric)
            .into_iter()
            .filter(shown)
            .map(|symbol| (symbol, 0))
            .rev()
            .collect();
        while let Some((symbol, depth)) = pending.pop() {
            let children: Vec<&Symbol> = analysis
                .children_by(&symbol.id, metric)
                .into_iter()
                .filter(shown)
                .collect();
            let open = (!children.is_empty()).then(|| {
                self.expanded.contains(&symbol.id) || (kept.is_some() && !children.is_empty())
            });
            rows.push(TreeRow {
                symbol,
                depth,
                open,
            });
            if open == Some(true) {
                pending.extend(children.into_iter().rev().map(|child| (child, depth + 1)));
            }
        }
        rows
    }

    /// The symbols Find lists, in order.
    #[must_use]
    pub fn found(&self) -> Vec<&Symbol> {
        let Some(analysis) = &self.analysis else {
            return Vec::new();
        };
        let (metric, lowest_first) = self.sorted_by();
        let order = if lowest_first {
            Order::Ascending
        } else {
            Order::Descending
        };
        let typing = self
            .prompt
            .as_ref()
            .is_some_and(|prompt| prompt.kind == PromptKind::Filter);
        let filter = match (&self.find.live, typing) {
            (Some(live), true) => Some(live),
            _ => self.lens.filter.as_ref().map(|filter| &filter.filter),
        };
        if let Some(filter) = filter {
            return analysis.search(filter, Some((metric, order)), None);
        }
        let mut all: Vec<&Symbol> = analysis
            .view()
            .symbols()
            .filter(|symbol| {
                analysis
                    .value(&Subject::Symbol(symbol.id.clone()), metric)
                    .is_some()
            })
            .collect();
        all.sort_by(|a, b| {
            let value = |symbol: &Symbol| {
                analysis
                    .value(&Subject::Symbol(symbol.id.clone()), metric)
                    .unwrap_or_default()
            };
            if lowest_first {
                value(a).total_cmp(&value(b))
            } else {
                value(b).total_cmp(&value(a))
            }
        });
        all
    }

    /// What `Tab` offers for the filter being written, each with what it means.
    #[must_use]
    pub fn completions(&self) -> Vec<(String, String)> {
        match &self.prompt {
            Some(prompt) if prompt.kind == PromptKind::Filter => completions(&prompt.text),
            _ => Vec::new(),
        }
    }

    /// `!`, `!!` or `@` for each symbol that has an alert or a cycle, or holds one.
    #[must_use]
    pub fn tree_marks(&self) -> HashMap<SymbolId, String> {
        let Some(analysis) = &self.analysis else {
            return HashMap::new();
        };
        let view = analysis.view();
        let mut severity: HashMap<SymbolId, Severity> = HashMap::new();
        for alert in analysis.alerts() {
            let Subject::Symbol(id) = &alert.subject else {
                continue;
            };
            let mut current = view.symbol(id);
            while let Some(symbol) = current {
                let entry = severity.entry(symbol.id.clone()).or_insert(alert.severity);
                if alert.severity > *entry {
                    *entry = alert.severity;
                }
                current = parent_of(symbol).and_then(|parent| view.symbol(parent));
            }
        }
        let mut marks: HashMap<SymbolId, String> = severity
            .into_iter()
            .map(|(id, severity)| {
                let mark = if severity == Severity::Error {
                    "!!"
                } else {
                    "!"
                };
                (id, mark.to_owned())
            })
            .collect();
        for symbol in view.symbols() {
            let in_cycle = symbol.kind == SymbolKind::Module
                && analysis
                    .value(&Subject::Symbol(symbol.id.clone()), MetricId::CycleSize)
                    .is_some();
            if in_cycle {
                let mark = marks.entry(symbol.id.clone()).or_default();
                if !mark.is_empty() {
                    mark.push(' ');
                }
                mark.push('@');
            }
        }
        marks
    }

    /// What a symbol uses and what uses it: each other end, the kind, the confidence, and
    /// whether it is used rather than using. What it uses comes first.
    #[must_use]
    pub fn relations_of(&self, id: &SymbolId) -> Vec<(SymbolId, RelationKind, Confidence, bool)> {
        let Some(analysis) = &self.analysis else {
            return Vec::new();
        };
        let view = analysis.view();
        let mut uses: Vec<(SymbolId, RelationKind, Confidence, bool)> = view
            .relations()
            .filter(|relation| &relation.from == id && &relation.to != id)
            .map(|relation| {
                (
                    relation.to.clone(),
                    relation.kind,
                    relation.confidence,
                    false,
                )
            })
            .collect();
        uses.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        let mut used_by: Vec<(SymbolId, RelationKind, Confidence, bool)> = view
            .relations()
            .filter(|relation| &relation.to == id && &relation.from != id)
            .map(|relation| {
                (
                    relation.from.clone(),
                    relation.kind,
                    relation.confidence,
                    true,
                )
            })
            .collect();
        used_by.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        uses.extend(used_by);
        uses
    }

    /// Where a symbol is written, as the editor wants it.
    fn place_of(&self, id: &SymbolId) -> Option<(String, u32)> {
        let analysis = self.analysis.as_ref()?;
        let symbol = analysis.view().symbol(id)?;
        if let Some(location) = &symbol.location {
            return Some((location.file.to_string(), location.span.start_line));
        }
        analysis
            .files()
            .iter()
            .find(|file| file.module.as_ref() == Some(id))
            .map(|file| (file.path.to_string(), 1))
    }

    /// The worst symbols by the metric, where it is measured, as the Overview ranks them.
    #[must_use]
    pub fn ranking(&self) -> Vec<Subject> {
        let Some(analysis) = &self.analysis else {
            return Vec::new();
        };
        let filter = self.lens.filter.as_ref().map(|filter| &filter.filter);
        analysis
            .ranking(self.lens.metric, filter, 8)
            .into_iter()
            .map(|(subject, _)| subject)
            .collect()
    }

    /// The tiles of the map: what is inside the focus, or inside its parent when the
    /// focus has nothing inside it.
    #[must_use]
    pub fn tiles(&self) -> Vec<&Symbol> {
        let Some(analysis) = &self.analysis else {
            return Vec::new();
        };
        let Some(holder) = self.map_holder() else {
            return Vec::new();
        };
        let kept = self.kept();
        let mut tiles: Vec<&Symbol> = analysis
            .view()
            .children(&holder)
            .filter(|symbol| kept.as_ref().is_none_or(|kept| kept.contains(&symbol.id)))
            .collect();
        tiles.sort_by(|a, b| a.id.cmp(&b.id));
        tiles
    }

    /// The symbol the map is drawn inside.
    fn map_holder(&self) -> Option<SymbolId> {
        let analysis = self.analysis.as_ref()?;
        let focus = self.focus.as_ref()?;
        if analysis.view().children(focus).next().is_some() {
            return Some(focus.clone());
        }
        analysis
            .view()
            .symbol(focus)
            .and_then(parent_of)
            .cloned()
            .or_else(|| Some(focus.clone()))
    }

    /// The level the diagram shows for the focus.
    fn natural_level(&self) -> Level {
        let kind = self
            .focus
            .as_ref()
            .and_then(|id| self.symbol(id))
            .map_or(SymbolKind::Package, |symbol| symbol.kind);
        match Level::of(kind) {
            Level::Package => Level::Module,
            other => other,
        }
    }

    /// The diagram around the focus, laid out in columns: what depends on the focus on
    /// the left, and what it depends on on the right.
    #[must_use]
    pub fn drawing(&self) -> Option<Drawing> {
        let analysis = self.analysis.as_ref()?;
        let level = self.diagram.level.unwrap_or_else(|| self.natural_level());
        let depth = self.diagram.depth.unwrap_or(1);
        let request = DiagramRequest {
            level,
            focus: self.focus.clone(),
            depth,
            relations: Vec::new(),
            overlay: Some(self.lens.metric),
            filter: self
                .lens
                .filter
                .as_ref()
                .map(|filter| filter.filter.clone()),
        };
        let view = analysis.diagram(&request);
        let placed = layout(&view, self.focus.as_ref());
        Some(Drawing {
            view,
            level,
            depth,
            placed,
        })
    }

    /// The row, tile or node under the cursor.
    #[must_use]
    pub fn selected(&self) -> Option<SymbolId> {
        match self.view {
            View::Explore | View::Overview | View::Alerts | View::Status => self.focus.clone(),
            View::Find => self
                .found()
                .get(self.find.index)
                .map(|symbol| symbol.id.clone()),
            View::Diagram => self.diagram.selected.clone().or_else(|| self.focus.clone()),
            View::Map => self.map_selected.clone(),
        }
    }

    // Changing it.

    /// Takes an analysis, or the reason there is none.
    pub fn analyzed(&mut self, result: Result<ProjectAnalysis, String>) {
        self.progress = None;
        self.stale = false;
        self.analyses += 1;
        match result {
            Ok(analysis) => {
                self.failure = None;
                let alerts_before = self.analysis.as_ref().map(|old| old.alerts().len());
                if alerts_before.is_some_and(|before| analysis.alerts().len() > before) {
                    self.found_alerts = Some(self.now);
                }
                self.lens.tests = analysis.tests() == TestCode::Included;
                if !self.lens.chosen {
                    self.lens.metric = if analysis.why_unavailable(MetricId::Hotspot).is_none() {
                        MetricId::Hotspot
                    } else {
                        MetricId::Cyclomatic
                    };
                }
                let roots: Vec<SymbolId> = analysis
                    .roots_by(MetricId::Lines)
                    .iter()
                    .map(|symbol| symbol.id.clone())
                    .collect();
                if self.expanded.is_empty() {
                    self.expanded.extend(roots.iter().cloned());
                }
                let before = self.focus.clone();
                let focus = before
                    .as_ref()
                    .and_then(|focus| analysis.nearest_visible(focus))
                    .map(|symbol| symbol.id.clone())
                    .or_else(|| roots.first().cloned());
                let tests_hid_it = before
                    .as_ref()
                    .is_some_and(|before| analysis.model().symbol(before).is_some());
                if let Some(before) = before.filter(|before| Some(before) != focus.as_ref())
                    && !tests_hid_it
                {
                    let name = before.as_str().rsplit("::").next().unwrap_or_default();
                    self.message = Some(format!("{name} is gone"));
                }
                self.analysis = Some(analysis);
                self.set_focus(focus);
                if let Some(start) = self.trail.places.first_mut()
                    && start.focus.is_none()
                {
                    start.focus.clone_from(&self.focus);
                }
            }
            Err(reason) => {
                self.failure = Some(reason);
                self.analysis = None;
                self.focus = None;
            }
        }
    }

    /// The settings were saved, or could not be.
    pub fn initialized(&mut self, result: Result<String, String>) {
        self.project.has_settings |= result.is_ok();
        self.message = Some(result.unwrap_or_else(|error| error));
    }

    /// Hears how the running analysis is going.
    pub fn heard(&mut self, heard: Heard) {
        let Some(running) = &mut self.progress else {
            return;
        };
        match heard {
            Heard::Step(step) => running.step = Some(step),
            Heard::Files(done, of) => running.files = (done, of),
            Heard::Partial(partial) => running.partial = Some(partial),
            Heard::Indexer(indexer) => running.indexer = Some(indexer),
        }
    }

    /// Whether precise analysis can run, so that it may be asked for.
    pub fn precise_checked(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => self.popup = Some(Popup::ConfirmPrecise),
            Err(reason) => self.message = Some(reason),
        }
    }

    /// Says something on the hint line.
    pub fn tell(&mut self, message: String) {
        self.message = Some(message);
    }

    /// The editor closed, or could not open.
    pub fn editor_closed(&mut self, result: Result<(), String>) {
        if let Err(reason) = result {
            self.message = Some(reason);
        }
    }

    /// Opens the branches down to a symbol, so that the tree shows it.
    fn reveal(&mut self, id: &SymbolId) {
        let mut ancestors = Vec::new();
        let mut current = self.symbol(id).and_then(parent_of).cloned();
        while let Some(parent) = current {
            current = self.symbol(&parent).and_then(parent_of).cloned();
            ancestors.push(parent);
        }
        self.expanded.extend(ancestors);
    }

    fn set_focus(&mut self, focus: Option<SymbolId>) {
        if let Some(id) = &focus {
            self.reveal(id);
        }
        if focus != self.focus {
            self.diagram.selected = None;
        }
        self.focus = focus;
    }

    fn place(&self) -> Place {
        Place {
            view: self.view,
            focus: self.focus.clone(),
            section: self.inspector,
        }
    }

    fn go_to(&mut self, place: Place) {
        self.switch_view(place.view);
        self.set_focus(place.focus);
        self.inspector = place.section;
    }

    /// Jumps somewhere, and records it on the trail.
    fn jump(&mut self, place: Place) {
        let here = self.place();
        if here == place {
            return;
        }
        let position = self.trail.position;
        if let Some(current) = self.trail.places.get_mut(position) {
            *current = here;
        }
        self.trail.places.truncate(position + 1);
        self.trail.places.push(place.clone());
        self.trail.position = self.trail.places.len() - 1;
        self.go_to(place);
    }

    fn back(&mut self, forward: bool) {
        let position = self.trail.position;
        let next = if forward {
            position + 1
        } else {
            match position.checked_sub(1) {
                Some(next) => next,
                None => return,
            }
        };
        let Some(place) = self.trail.places.get(next).cloned() else {
            return;
        };
        let here = self.place();
        if let Some(current) = self.trail.places.get_mut(position) {
            *current = here;
        }
        self.trail.position = next;
        self.go_to(place);
    }

    /// Jumps to a symbol, in this view if it can show it, or else in Explore.
    fn jump_to_symbol(&mut self, id: SymbolId, section: Option<Section>) {
        let kind = self.symbol(&id).map(|symbol| symbol.kind);
        let view = if kind.is_some_and(|kind| self.view.shows(kind)) {
            self.view
        } else {
            View::Explore
        };
        self.jump(Place {
            view,
            focus: Some(id),
            section,
        });
    }

    fn switch_view(&mut self, view: View) {
        if view != self.view {
            self.pane = 0;
            self.view = view;
        }
    }

    /// Handles one input.
    pub fn update(&mut self, input: Input) {
        match input {
            Input::Tick(now) => self.now = now,
            Input::Resize { columns, rows } => {
                self.columns = columns;
                self.rows = rows;
            }
            Input::FilesChanged => {
                if !self.analyzing() {
                    self.stale = true;
                }
            }
            Input::Key(key) => {
                self.message = None;
                self.key(key);
            }
            Input::Mouse { kind, column, row } => {
                self.message = None;
                self.mouse(kind, column, row);
            }
        }
    }

    fn mouse(&mut self, kind: MouseKind, column: u16, row: u16) {
        let target = self
            .hits
            .iter()
            .rev()
            .find(|(area, _)| {
                column >= area.x
                    && column < area.x + area.width
                    && row >= area.y
                    && row < area.y + area.height
            })
            .map(|(_, target)| target.clone());
        let (Some(target), MouseKind::Click) = (target, kind) else {
            return;
        };
        match target {
            Target::Menu(key) => {
                self.popup = None;
                self.space_action(key);
            }
            Target::Tab(view) => {
                self.popup = None;
                self.switch_view(view);
            }
            Target::Symbol(id) => self.select(id),
            Target::Column(metric) => self.sort_by(metric),
        }
    }

    /// Selects a symbol where the view shows it.
    fn select(&mut self, id: SymbolId) {
        match self.view {
            View::Diagram => self.diagram.selected = Some(id),
            View::Map => self.map_selected = Some(id),
            View::Find => {
                if let Some(index) = self.found().iter().position(|symbol| symbol.id == id) {
                    self.find.index = index;
                }
                self.set_focus(Some(id));
            }
            View::Explore | View::Overview | View::Alerts | View::Status => {
                self.set_focus(Some(id));
            }
        }
    }

    fn key(&mut self, key: Key) {
        if self.prompt.is_some() {
            self.prompt_key(key);
            return;
        }
        if self.popup.is_some() {
            self.popup_key(key);
            return;
        }
        if let Some((prefix, _)) = self.pending.take() {
            self.second_key(prefix, key);
            self.count = None;
            return;
        }
        if let Key::Char(digit @ '0'..='9') = key
            && (digit != '0' || self.count.is_some())
        {
            let value = digit.to_digit(10).unwrap_or_default();
            self.count = Some(
                self.count
                    .unwrap_or(0)
                    .saturating_mul(10)
                    .saturating_add(value),
            );
            return;
        }
        self.normal_key(key);
        if self.pending.is_none() {
            self.count = None;
        }
    }

    fn times(&self) -> usize {
        usize::try_from(self.count.unwrap_or(1)).unwrap_or(1).max(1)
    }

    fn open_prompt(&mut self, kind: PromptKind, text: String) {
        self.prompt = Some(Prompt {
            kind,
            text,
            error: None,
        });
    }

    fn normal_key(&mut self, key: Key) {
        match key {
            Key::Char(' ') => self.popup = Some(Popup::SpaceMenu),
            Key::Char(prefix @ ('g' | 'z' | ']' | '[' | 'm' | '\'' | 'y'))
            | Key::Ctrl(prefix @ 'w') => {
                let key = if prefix == 'w' {
                    Key::Ctrl('w')
                } else {
                    Key::Char(prefix)
                };
                self.pending = Some((key, self.now));
            }
            Key::Char(':') => self.open_prompt(PromptKind::Command, String::new()),
            Key::Char('/') => self.open_prompt(PromptKind::Search, String::new()),
            Key::Ctrl('p') => self.open_prompt(PromptKind::Jump, String::new()),
            Key::Char('f') => {
                let text = self
                    .lens
                    .filter
                    .as_ref()
                    .map(|filter| filter.text.clone())
                    .unwrap_or_default();
                self.open_prompt(PromptKind::Filter, text);
            }
            Key::Char('F') => self.lens.filter = None,
            Key::Char('s') if self.view == View::Find => {
                self.popup = Some(Popup::SortPicker {
                    typed: String::new(),
                });
            }
            Key::Char('j') | Key::Down if self.pane() == Some(Pane::Inspector) => {
                let count = self.inspector_rows();
                self.inspector_row = moved(self.inspector_row, 1, count);
            }
            Key::Char('k') | Key::Up if self.pane() == Some(Pane::Inspector) => {
                self.inspector_row = self.inspector_row.saturating_sub(1);
            }
            Key::Enter if self.pane() == Some(Pane::Inspector) => self.follow_inspector_row(),
            Key::Char('M') => {
                self.popup = Some(Popup::MetricPicker {
                    typed: String::new(),
                });
            }
            Key::Char('K') => {
                self.inspector = match self.inspector {
                    Some(_) => None,
                    None => Some(Section::Metrics),
                };
            }
            Key::Char('H') => self.inspector = self.inspector.map(|section| section.step(false)),
            Key::Char('L') => self.inspector = self.inspector.map(|section| section.step(true)),
            Key::Char('q') => {
                if self.inspector.is_some() {
                    self.inspector = None;
                }
            }
            Key::Char('?') => self.popup = Some(Popup::Help { all: false }),
            Key::Tab => {
                let count = self.panes().len().max(1);
                self.pane = (self.pane.min(count - 1) + 1) % count;
            }
            Key::Ctrl('o') => self.back(false),
            Key::Ctrl('n') => self.back(true),
            Key::Char('n') => self.search_again(true),
            Key::Char('N') => self.search_again(false),
            Key::Char('j') | Key::Down => self.step(isize::try_from(self.times()).unwrap_or(1)),
            Key::Char('k') | Key::Up => self.step(-isize::try_from(self.times()).unwrap_or(1)),
            Key::Ctrl('d') => self.step(self.half_page()),
            Key::Ctrl('u') => self.step(-self.half_page()),
            Key::Char('G') => self.step(isize::MAX / 2),
            Key::Char('h') | Key::Left => self.sideways(false),
            Key::Char('l') | Key::Right => self.sideways(true),
            Key::Enter => self.enter(),
            Key::Char('-') => self.up_a_level(),
            Key::Char('c') if self.view == View::Diagram => {
                if let Some(selected) = self.selected() {
                    self.jump(Place {
                        view: View::Diagram,
                        focus: Some(selected),
                        section: self.inspector,
                    });
                }
            }
            Key::Char('<') if self.view == View::Diagram => {
                let depth = self.diagram.depth.unwrap_or(1);
                self.diagram.depth = Some(depth.saturating_sub(1));
            }
            Key::Char('>') if self.view == View::Diagram => {
                let depth = self.diagram.depth.unwrap_or(1);
                self.diagram.depth = Some(depth.saturating_add(1));
            }
            _ => {}
        }
    }

    fn half_page(&self) -> isize {
        isize::try_from(self.rows.saturating_sub(4) / 2)
            .unwrap_or(1)
            .max(1)
    }

    fn second_key(&mut self, prefix: Key, key: Key) {
        match (prefix, key) {
            (Key::Char('g'), Key::Char('g')) => self.step(-(isize::MAX / 2)),
            (Key::Char('g'), Key::Char('t')) => match self.count {
                Some(place) => {
                    let index = usize::try_from(place).unwrap_or(1).saturating_sub(1);
                    if let Some(view) = View::ALL.get(index) {
                        self.switch_view(*view);
                    }
                }
                None => self.next_view(true),
            },
            (Key::Char('g'), Key::Char('T')) => self.next_view(false),
            (Key::Char('g'), Key::Char('d')) => self.why(),
            (Key::Char('g'), Key::Char('r')) => {
                if let Some(selected) = self.selected() {
                    self.jump(Place {
                        view: View::Diagram,
                        focus: Some(selected),
                        section: Some(Section::Impact),
                    });
                }
            }
            (Key::Char('g'), Key::Char('c')) => {
                if let Some(selected) = self.selected() {
                    self.jump(Place {
                        view: View::Explore,
                        focus: Some(selected),
                        section: Some(Section::Clones),
                    });
                }
            }
            (Key::Char('g'), Key::Char('f')) => self.open_in_editor(),
            (Key::Char('y'), Key::Char(what @ ('y' | 'f' | 'c'))) => self.copy(what),
            (Key::Char('z'), Key::Char('a')) => self.toggle_fold(),
            (Key::Char('z'), Key::Char('R')) => self.open_all(),
            (Key::Char('z'), Key::Char('M')) => {
                self.expanded.clear();
                let root = self.focus.as_ref().map(|focus| self.root_of(focus));
                self.set_focus(root);
                self.expanded.clear();
            }
            (Key::Char(']'), Key::Char('m')) => self.next_metric(true),
            (Key::Char('['), Key::Char('m')) => self.next_metric(false),
            (Key::Char('m'), Key::Char(letter)) if letter.is_ascii_lowercase() => {
                if let Some(selected) = self.selected() {
                    self.marks.insert(letter, selected);
                }
            }
            (Key::Char('\''), Key::Char(letter)) => match self.marks.get(&letter).cloned() {
                Some(id) => self.jump_to_symbol(id, None),
                None => self.message = Some(format!("Mark not set: {letter}")),
            },
            (Key::Ctrl('w'), Key::Char('l' | 'j')) => {
                let count = self.panes().len().max(1);
                self.pane = (self.pane + 1).min(count - 1);
            }
            (Key::Ctrl('w'), Key::Char('h' | 'k')) => self.pane = self.pane.saturating_sub(1),
            _ => {}
        }
    }

    fn root_of(&self, id: &SymbolId) -> SymbolId {
        let mut current = id.clone();
        while let Some(parent) = self.symbol(&current).and_then(parent_of).cloned() {
            current = parent;
        }
        current
    }

    fn next_view(&mut self, forward: bool) {
        let count = View::ALL.len();
        let here = self.view.position();
        let next = if forward {
            (here + 1) % count
        } else {
            (here + count - 1) % count
        };
        if let Some(view) = View::ALL.get(next) {
            self.switch_view(*view);
        }
    }

    fn next_metric(&mut self, forward: bool) {
        let Some(analysis) = &self.analysis else {
            return;
        };
        let available: Vec<MetricId> = OVERVIEW_METRICS
            .into_iter()
            .filter(|metric| analysis.why_unavailable(*metric).is_none())
            .collect();
        let count = available.len();
        if count == 0 {
            return;
        }
        let next = match available
            .iter()
            .position(|metric| *metric == self.lens.metric)
        {
            Some(here) if forward => (here + 1) % count,
            Some(here) => (here + count - 1) % count,
            None if forward => 0,
            None => count - 1,
        };
        if let Some(metric) = available.get(next) {
            self.set_metric(*metric);
        }
    }

    fn set_metric(&mut self, metric: MetricId) {
        self.lens.metric = metric;
        self.lens.chosen = true;
    }

    /// Opens the Why of the selection, or of its worst function.
    fn why(&mut self) {
        let Some(selected) = self.selected() else {
            return;
        };
        let target = self
            .analysis
            .as_ref()
            .and_then(|analysis| analysis.worst_function_in(&selected, self.lens.metric))
            .map(|symbol| symbol.id.clone());
        let Some(target) = target else {
            self.message = Some(format!("No function in {}", self.name_of(&selected)));
            return;
        };
        let message = (target != selected).then(|| format!("worst in {}", self.name_of(&selected)));
        self.jump(Place {
            view: View::Explore,
            focus: Some(target),
            section: Some(Section::Why),
        });
        self.message = message;
    }

    /// Moves the selection by some rows, or nodes.
    fn step(&mut self, by: isize) {
        match self.view {
            View::Explore => {
                let ids: Vec<SymbolId> = self
                    .tree()
                    .iter()
                    .map(|row| row.symbol.id.clone())
                    .collect();
                let here = self
                    .focus
                    .as_ref()
                    .and_then(|focus| ids.iter().position(|id| id == focus))
                    .unwrap_or_default();
                if let Some(id) = ids.get(moved(here, by, ids.len())) {
                    self.set_focus(Some(id.clone()));
                }
            }
            View::Find => {
                let count = self.found().len();
                self.find.index = moved(self.find.index, by, count);
                let selected = self.selected();
                if selected.is_some() {
                    self.set_focus(selected);
                }
            }
            View::Overview => {
                if self.pane() == Some(Pane::Ranking) {
                    let count = self.ranking().len();
                    self.ranking_index = moved(self.ranking_index, by, count);
                } else {
                    let here = OVERVIEW_METRICS
                        .iter()
                        .position(|metric| *metric == self.lens.metric)
                        .unwrap_or_default();
                    let next = moved(here, by, OVERVIEW_METRICS.len());
                    if let Some(metric) = OVERVIEW_METRICS.get(next) {
                        self.set_metric(*metric);
                    }
                }
            }
            View::Diagram => self.diagram_step(0, by.signum()),
            View::Map => self.map_step(by.signum()),
            View::Alerts | View::Status => {}
        }
    }

    fn sideways(&mut self, right: bool) {
        match self.view {
            View::Explore => {
                let Some(focus) = self.focus.clone() else {
                    return;
                };
                let open = self
                    .tree()
                    .iter()
                    .find(|row| row.symbol.id == focus)
                    .and_then(|row| row.open);
                match (right, open) {
                    (true, Some(false)) => {
                        self.expanded.insert(focus);
                    }
                    (false, Some(true)) => {
                        self.expanded.remove(&focus);
                    }
                    (false, _) => {
                        let parent = self.symbol(&focus).and_then(parent_of).cloned();
                        if parent.is_some() {
                            self.set_focus(parent);
                        }
                    }
                    (true, _) => {}
                }
            }
            View::Diagram => self.diagram_step(if right { 1 } else { -1 }, 0),
            View::Map => self.map_step(if right { 1 } else { -1 }),
            _ => {}
        }
    }

    fn toggle_fold(&mut self) {
        if let Some(focus) = self.focus.clone()
            && !self.expanded.remove(&focus)
        {
            self.expanded.insert(focus);
        }
    }

    fn open_all(&mut self) {
        if let Some(analysis) = &self.analysis {
            let all: Vec<SymbolId> = analysis
                .view()
                .symbols()
                .map(|symbol| symbol.id.clone())
                .collect();
            self.expanded.extend(all);
        }
    }

    fn diagram_step(&mut self, columns: isize, rows: isize) {
        let Some(drawing) = self.drawing() else {
            return;
        };
        let current = self.selected();
        let here = drawing
            .placed
            .iter()
            .find(|placed| Some(&placed.id) == current.as_ref())
            .or_else(|| drawing.placed.first());
        let Some(here) = here else {
            return;
        };
        let column = here.column.saturating_add_signed(columns);
        let row = here.row.saturating_add_signed(rows);
        let next = drawing
            .placed
            .iter()
            .filter(|placed| placed.column == column && (columns != 0 || placed.row == row))
            .min_by_key(|placed| placed.row.abs_diff(row));
        if let Some(next) = next {
            self.diagram.selected = Some(next.id.clone());
        }
    }

    fn map_step(&mut self, by: isize) {
        let ids: Vec<SymbolId> = self
            .tiles()
            .iter()
            .map(|symbol| symbol.id.clone())
            .collect();
        let here = self
            .map_selected
            .as_ref()
            .and_then(|selected| ids.iter().position(|id| id == selected));
        let next = here.map_or(0, |here| moved(here, by, ids.len()));
        if let Some(id) = ids.get(next) {
            self.map_selected = Some(id.clone());
        }
    }

    fn enter(&mut self) {
        match self.view {
            View::Explore => {
                if let Some(focus) = self.focus.clone() {
                    self.expanded.insert(focus);
                }
            }
            View::Diagram => {
                let Some(selected) = self.selected() else {
                    return;
                };
                if self
                    .symbol(&selected)
                    .is_some_and(|symbol| symbol.kind.is_type())
                {
                    return;
                }
                let lower = match self.diagram.level.unwrap_or_else(|| self.natural_level()) {
                    Level::Package => Level::Module,
                    Level::Module => Level::Type,
                    Level::Type | Level::Member => Level::Member,
                };
                self.diagram.drills.push(Drill {
                    focus: self.focus.clone(),
                    depth: self.diagram.depth.unwrap_or(1),
                });
                self.diagram.level = Some(lower);
                self.diagram.depth = Some(0);
                self.jump(Place {
                    view: View::Diagram,
                    focus: Some(selected),
                    section: self.inspector,
                });
            }
            View::Map => {
                let Some(selected) = self.map_selected.clone() else {
                    return;
                };
                let has_inside = self
                    .analysis
                    .as_ref()
                    .is_some_and(|analysis| analysis.view().children(&selected).next().is_some());
                if has_inside {
                    self.map_selected = None;
                    self.jump(Place {
                        view: View::Map,
                        focus: Some(selected),
                        section: self.inspector,
                    });
                }
            }
            View::Overview => {
                if self.pane() == Some(Pane::Ranking)
                    && let Some(Subject::Symbol(id)) =
                        self.ranking().get(self.ranking_index).cloned()
                {
                    self.jump_to_symbol(id, None);
                }
            }
            View::Find => {
                if let Some(selected) = self.selected() {
                    self.jump(Place {
                        view: View::Explore,
                        focus: Some(selected),
                        section: self.inspector,
                    });
                }
            }
            View::Alerts | View::Status => {}
        }
    }

    fn up_a_level(&mut self) {
        if self.view == View::Diagram
            && let Some(drill) = self.diagram.drills.pop()
        {
            self.diagram.level = None;
            self.diagram.depth = Some(drill.depth);
            self.set_focus(drill.focus);
            return;
        }
        let Some(focus) = self.focus.clone() else {
            return;
        };
        let Some(parent) = self.symbol(&focus).and_then(parent_of).cloned() else {
            return;
        };
        if self.view == View::Map {
            self.map_selected = Some(focus);
        }
        self.set_focus(Some(parent));
    }

    /// The names the current view shows, with their symbols, in order.
    fn searchable(&self) -> Vec<(SymbolId, String)> {
        let pair = |symbol: &Symbol| (symbol.id.clone(), symbol.name.clone());
        match self.view {
            View::Explore => self.tree().iter().map(|row| pair(row.symbol)).collect(),
            View::Find => self.found().into_iter().map(pair).collect(),
            View::Map => self.tiles().into_iter().map(pair).collect(),
            View::Diagram => self
                .drawing()
                .map(|drawing| {
                    drawing
                        .placed
                        .iter()
                        .map(|placed| (placed.id.clone(), self.name_of(&placed.id)))
                        .collect()
                })
                .unwrap_or_default(),
            View::Overview | View::Alerts | View::Status => Vec::new(),
        }
    }

    fn search_from(&mut self, pattern: &str, forward: bool) {
        let names = self.searchable();
        let count = names.len();
        let here = self
            .selected()
            .and_then(|selected| names.iter().position(|(id, _)| *id == selected));
        let found = (1..=count).find_map(|offset| {
            let index = match (here, forward) {
                (Some(here), true) => (here + offset) % count,
                (Some(here), false) => (here + count * 2 - offset) % count,
                (None, true) => offset - 1,
                (None, false) => count - offset,
            };
            names
                .get(index)
                .filter(|(_, name)| name.contains(pattern))
                .map(|(id, _)| id.clone())
        });
        match found {
            Some(id) => self.select(id),
            None => self.message = Some(format!("Pattern not found: {pattern}")),
        }
    }

    fn search_again(&mut self, forward: bool) {
        if let Some(pattern) = self.search.clone() {
            self.search_from(&pattern, forward);
        }
    }

    fn popup_key(&mut self, key: Key) {
        let Some(popup) = self.popup.take() else {
            return;
        };
        match popup {
            Popup::SpaceMenu => {
                if let Key::Char(letter) = key {
                    self.space_action(letter);
                }
            }
            Popup::Help { .. } => {
                if !matches!(key, Key::Esc | Key::Char('q' | '?')) {
                    self.popup = Some(popup);
                }
            }
            Popup::ConfirmPrecise => {
                if matches!(key, Key::Char('y' | 'Y')) {
                    self.analyze(true);
                } else if !matches!(key, Key::Esc | Key::Char('n' | 'N' | 'q')) {
                    self.popup = Some(Popup::ConfirmPrecise);
                }
            }
            Popup::SortPicker { mut typed } => match key {
                Key::Esc => {}
                Key::Enter => {
                    if let Some(metric) = picked(&typed).first() {
                        self.sort_by(*metric);
                    }
                }
                Key::Backspace => {
                    typed.pop();
                    self.popup = Some(Popup::SortPicker { typed });
                }
                Key::Char(character) => {
                    typed.push(character);
                    self.popup = Some(Popup::SortPicker { typed });
                }
                _ => self.popup = Some(Popup::SortPicker { typed }),
            },
            Popup::MetricPicker { mut typed } => match key {
                Key::Esc => {}
                Key::Enter => {
                    if let Some(metric) = picked(&typed).first() {
                        self.set_metric(*metric);
                    }
                }
                Key::Backspace => {
                    typed.pop();
                    self.popup = Some(Popup::MetricPicker { typed });
                }
                Key::Char(character) => {
                    typed.push(character);
                    self.popup = Some(Popup::MetricPicker { typed });
                }
                _ => self.popup = Some(Popup::MetricPicker { typed }),
            },
        }
    }

    fn space_action(&mut self, letter: char) {
        if let Some(view) = View::by_letter(letter) {
            self.switch_view(view);
            return;
        }
        match letter {
            'r' => self.analyze(false),
            'p' => self.effects.push(Effect::CheckPrecise),
            't' => {
                self.lens.tests = !self.lens.tests;
                self.tests_override = Some(self.lens.tests);
                self.analyze(false);
            }
            'x' => {
                self.message = Some("Export this view with :export FORMAT PATH".to_owned());
            }
            '?' => self.popup = Some(Popup::Help { all: true }),
            'q' => self.quit(),
            _ => {}
        }
    }

    fn analyze(&mut self, precise: bool) {
        self.progress = Some(Running {
            since: self.now,
            precise,
            ..Running::default()
        });
        self.effects.push(Effect::Analyze {
            tests: self.tests_override,
            precise,
        });
    }

    fn quit(&mut self) {
        self.running = false;
        self.effects.push(Effect::Quit);
    }

    fn prompt_key(&mut self, key: Key) {
        let Some(mut prompt) = self.prompt.take() else {
            return;
        };
        match key {
            Key::Esc => self.find.live = None,
            Key::Enter => self.submit(prompt),
            Key::Backspace => {
                prompt.text.pop();
                prompt.error = None;
                self.prompt = Some(prompt);
                self.check_live();
            }
            Key::Ctrl('u') => {
                prompt.text.clear();
                prompt.error = None;
                self.prompt = Some(prompt);
                self.check_live();
            }
            Key::Tab if prompt.kind == PromptKind::Filter => {
                if let Some((word, _)) = completions(&prompt.text).first() {
                    let kept = prompt
                        .text
                        .rfind(char::is_whitespace)
                        .map_or("", |space| prompt.text.get(..=space).unwrap_or_default())
                        .to_owned();
                    let kept = if prompt.text.ends_with(char::is_whitespace) {
                        prompt.text.clone()
                    } else {
                        kept
                    };
                    prompt.text = format!("{kept}{word}");
                }
                self.prompt = Some(prompt);
                self.check_live();
            }
            Key::Up | Key::Down if prompt.kind == PromptKind::Filter => {
                let count = self.find.history.len();
                if count > 0 {
                    let next = match (self.find.recalled, key) {
                        (None, _) => count - 1,
                        (Some(at), Key::Up) => at.saturating_sub(1),
                        (Some(at), _) => (at + 1).min(count - 1),
                    };
                    self.find.recalled = Some(next);
                    if let Some(text) = self.find.history.get(next) {
                        prompt.text.clone_from(text);
                    }
                }
                self.prompt = Some(prompt);
                self.check_live();
            }
            Key::Tab if prompt.kind == PromptKind::Command => {
                if let Some(command) = COMMANDS
                    .iter()
                    .find(|command| command.starts_with(prompt.text.as_str()))
                {
                    (*command).clone_into(&mut prompt.text);
                }
                self.prompt = Some(prompt);
            }
            Key::Char(character) => {
                prompt.text.push(character);
                prompt.error = None;
                self.prompt = Some(prompt);
                self.check_live();
            }
            _ => self.prompt = Some(prompt),
        }
    }

    /// In Find, checks the filter as it is typed: results follow it while it can be
    /// understood, and the reason shows while it cannot.
    fn check_live(&mut self) {
        if self.view != View::Find {
            return;
        }
        let Some(prompt) = self
            .prompt
            .as_mut()
            .filter(|prompt| prompt.kind == PromptKind::Filter)
        else {
            return;
        };
        let text = prompt.text.trim();
        if text.is_empty() {
            self.find.live = None;
            return;
        }
        match Filter::parse(text) {
            Ok(filter) => {
                self.find.live = Some(filter);
                self.find.index = 0;
            }
            Err(error) => prompt.error = Some(error.to_string()),
        }
    }

    /// Sorts Find by a metric, or turns the order around if it already is.
    fn sort_by(&mut self, metric: MetricId) {
        let (current, lowest_first) = self.sorted_by();
        let lowest_first = if current == metric {
            !lowest_first
        } else {
            false
        };
        self.find.sort = Some((metric, lowest_first));
        self.set_metric(metric);
    }

    fn open_in_editor(&mut self) {
        if !self.project.editor {
            self.message = Some("Set $EDITOR to open files.".to_owned());
            return;
        }
        let Some((path, line)) = self.selected().and_then(|id| self.place_of(&id)) else {
            return;
        };
        self.opened = Some(format!("{path}:{line}"));
        self.effects.push(Effect::Open { path, line });
    }

    /// How many rows the open inspector section lists.
    fn inspector_rows(&self) -> usize {
        match (self.inspector, self.selected()) {
            (Some(Section::Relations), Some(id)) => self.relations_of(&id).len(),
            _ => 0,
        }
    }

    /// Jumps to what the selected row of the inspector names.
    fn follow_inspector_row(&mut self) {
        let Some(id) = self.selected() else {
            return;
        };
        if self.inspector == Some(Section::Relations)
            && let Some((other, ..)) = self.relations_of(&id).get(self.inspector_row).cloned()
        {
            self.inspector_row = 0;
            self.jump_to_symbol(other, self.inspector);
        }
    }

    /// Copies the selection, the filter, or the search as a command.
    fn copy(&mut self, what: char) {
        let filter = self.lens.filter.as_ref().map(|filter| filter.text.clone());
        let text = match what {
            'y' => self.selected().map(|id| id.to_string()),
            'f' => filter,
            _ => {
                let (metric, lowest_first) = self.sorted_by();
                let order = if lowest_first { "asc" } else { "desc" };
                let filter = filter.unwrap_or_else(|| format!("{} >= 0", metric.name()));
                Some(format!(
                    "cyclops query '{filter}' --sort '{} {order}'",
                    metric.name()
                ))
            }
        };
        if let Some(text) = text {
            self.copied = Some(text.clone());
            self.effects.push(Effect::Copy(text));
        }
    }

    fn submit(&mut self, mut prompt: Prompt) {
        let text = prompt.text.trim().to_owned();
        match prompt.kind {
            PromptKind::Command => self.command(&text),
            PromptKind::Filter => {
                if text.is_empty() {
                    self.lens.filter = None;
                    return;
                }
                match Filter::parse(&text) {
                    Ok(filter) => {
                        if self.find.history.last() != Some(&text) {
                            self.find.history.push(text.clone());
                        }
                        self.find.recalled = None;
                        self.find.live = None;
                        self.lens.filter = Some(LensFilter { text, filter });
                        self.find.index = 0;
                    }
                    Err(error) => {
                        prompt.error = Some(error.to_string());
                        self.prompt = Some(prompt);
                    }
                }
            }
            PromptKind::Search => {
                if !text.is_empty() {
                    self.search = Some(text.clone());
                    self.search_from(&text, true);
                }
            }
            PromptKind::Jump => {
                let found = self.analysis.as_ref().and_then(|analysis| {
                    analysis
                        .find_by_name(&text)
                        .first()
                        .map(|symbol| symbol.id.clone())
                });
                match found {
                    Some(id) => self.jump_to_symbol(id, None),
                    None => self.message = Some(format!("No symbol matches: {text}")),
                }
            }
        }
    }

    fn command(&mut self, text: &str) {
        match text {
            "q" | "q!" | "quit" => self.quit(),
            "w" | "w user" => self.message = Some("There is no rule to save.".to_owned()),
            "init" => self.effects.push(Effect::Init),
            "help" => self.popup = Some(Popup::Help { all: true }),
            "" => {}
            other => {
                if let Some(value) = other.strip_prefix("set bg=") {
                    match ThemeChoice::parse(value) {
                        Some(choice) => self.colors.session = Some(choice),
                        None => self.message = Some(format!("Unknown theme: {value}")),
                    }
                } else if other == "export" || other.starts_with("export ") {
                    self.message = Some("Exporting from here is not available yet.".to_owned());
                } else {
                    self.message = Some(format!("Unknown command: {other}"));
                }
            }
        }
    }

    /// What a metric is worth on a symbol, as the interface prints it.
    #[must_use]
    pub fn shown_value(&self, id: &SymbolId, metric: MetricId) -> String {
        self.analysis
            .as_ref()
            .and_then(|analysis| analysis.value(&Subject::Symbol(id.clone()), metric))
            .map_or_else(
                || "n/a".to_owned(),
                |value| metric.descriptor().format(value),
            )
    }

    /// The label of the lens's metric.
    #[must_use]
    pub const fn metric_label(&self) -> &'static str {
        metric_label(self.lens.metric)
    }
}

/// Moves an index by some steps, staying within `count`.
fn moved(here: usize, by: isize, count: usize) -> usize {
    if count == 0 {
        return 0;
    }
    here.saturating_add_signed(by).min(count - 1)
}

/// The metrics the picker offers for what was typed, in the order of the catalog.
#[must_use]
pub fn picked(typed: &str) -> Vec<MetricId> {
    let typed = typed.trim().to_lowercase();
    MetricId::ALL
        .into_iter()
        .filter(|metric| metric.name().contains(&typed) || metric_label(*metric).contains(&typed))
        .collect()
}

/// The entries of the Space menu, by group.
#[must_use]
pub fn space_menu() -> [(&'static str, Vec<(char, &'static str)>); 3] {
    [
        (
            "Go to",
            View::ALL
                .iter()
                .map(|view| (view.letter(), view.name()))
                .collect(),
        ),
        (
            "Project",
            vec![
                ('r', "Refresh what changed"),
                ('p', "Precise analysis"),
                ('t', "Test code"),
                ('x', "Export this view"),
            ],
        ),
        ("Session", vec![('?', "All keys"), ('q', "Quit")]),
    ]
}

/// What can follow a prefix key.
#[must_use]
pub fn hints(prefix: char) -> &'static [(char, &'static str)] {
    match prefix {
        'g' => &[
            ('g', "first"),
            ('t', "next view"),
            ('T', "previous view"),
            ('d', "why"),
            ('r', "impact"),
            ('c', "clones"),
            ('f', "open in the editor"),
        ],
        'z' => &[('a', "fold"), ('R', "open all"), ('M', "close all")],
        ']' => &[
            ('m', "next metric"),
            ('a', "next alert"),
            ('c', "next cycle"),
            ('d', "next clone"),
            ('p', "next problem"),
        ],
        '[' => &[
            ('m', "previous metric"),
            ('a', "previous alert"),
            ('c', "previous cycle"),
            ('d', "previous clone"),
            ('p', "previous problem"),
        ],
        _ => &[],
    }
}

/// Places the nodes of a diagram in columns. Around a focus, what depends on it goes to
/// the left and what it depends on to the right. Without one, each node sits one column
/// right of what depends on it.
fn layout(view: &DiagramView, focus: Option<&SymbolId>) -> Vec<Placed> {
    let ids: Vec<&SymbolId> = view.nodes.iter().map(|node| &node.id).collect();
    let mut column: BTreeMap<&SymbolId, isize> = BTreeMap::new();
    if let Some(focus) = focus.filter(|focus| ids.contains(focus)) {
        column.insert(focus, 0);
        let mut frontier = vec![focus];
        while !frontier.is_empty() {
            let mut next = Vec::new();
            for here in frontier {
                let at = column.get(here).copied().unwrap_or_default();
                for edge in &view.edges {
                    let (other, offset) = if &edge.from == here {
                        (&edge.to, 1)
                    } else if &edge.to == here {
                        (&edge.from, -1)
                    } else {
                        continue;
                    };
                    if !column.contains_key(other) {
                        column.insert(other, at + offset);
                        next.push(other);
                    }
                }
            }
            frontier = next;
        }
    }
    for id in &ids {
        column.entry(id).or_insert(0);
    }
    if focus.is_none_or(|focus| !ids.contains(&focus)) {
        for _ in 0..ids.len() {
            for edge in &view.edges {
                let from = column.get(&edge.from).copied().unwrap_or_default();
                if let Some(to) = column.get_mut(&edge.to)
                    && *to <= from
                {
                    *to = from + 1;
                }
            }
        }
    }
    let lowest = column.values().copied().min().unwrap_or_default();
    let mut rows: BTreeMap<usize, usize> = BTreeMap::new();
    let mut placed: Vec<Placed> = ids
        .iter()
        .map(|id| {
            let at = column.get(id).copied().unwrap_or_default() - lowest;
            let column = usize::try_from(at).unwrap_or_default();
            let row = rows.entry(column).or_insert(0);
            let placed = Placed {
                id: (*id).clone(),
                column,
                row: *row,
            };
            *row += 1;
            placed
        })
        .collect();
    placed.sort_by_key(|placed| (placed.column, placed.row));
    placed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::screen_text;
    use crate::input::Gesture;
    use crate::input::parse_gestures;
    use crate::render;
    use cyclops_application::diagram::{Edge, Node};
    use cyclops_domain::relation::{Confidence, RelationKind};

    fn startup(columns: u16, rows: u16) -> Startup {
        Startup {
            columns,
            rows,
            theme: ThemeChoice::Auto,
            background: None,
            no_color: false,
            initialized: true,
            editor: false,
        }
    }

    /// An interface whose first analysis is over, so that it waits for keys.
    fn app() -> App {
        let mut app = App::new(startup(120, 36));
        app.take_effects();
        app.analyzed(Err("not a project".to_owned()));
        app
    }

    fn press(app: &mut App, keys: &str) {
        for gesture in parse_gestures(keys).unwrap() {
            if let Gesture::Key(key) = gesture {
                app.update(Input::Key(key));
            }
        }
    }

    #[test]
    fn it_opens_on_overview_and_asks_for_an_analysis() {
        let mut app = App::new(startup(120, 36));
        assert_eq!(app.view(), View::Overview);
        assert_eq!(app.mode(), Mode::Analyzing);
        assert_eq!(app.trail(), (1, 1));
        assert_eq!(
            app.take_effects(),
            vec![Effect::Analyze {
                tests: None,
                precise: false
            }]
        );
        assert!(!app.has_effects());
    }

    #[test]
    fn views_go_by_letter_by_step_and_by_place() {
        let mut app = app();
        press(&mut app, "<Space>d");
        assert_eq!(app.view(), View::Diagram);
        press(&mut app, "gt");
        assert_eq!(app.view(), View::Find);
        press(&mut app, "<Space>sgt");
        assert_eq!(app.view(), View::Overview);
        press(&mut app, "gT");
        assert_eq!(app.view(), View::Status);
        press(&mut app, "3gt");
        assert_eq!(app.view(), View::Explore);
    }

    #[test]
    fn prompts_set_the_mode_and_esc_leaves_them() {
        let mut app = app();
        press(&mut app, ":");
        assert_eq!(app.mode(), Mode::Command);
        press(&mut app, "abc<Esc>");
        assert_eq!(app.mode(), Mode::Normal);
        press(&mut app, "f");
        assert_eq!(app.mode(), Mode::Filter);
        press(&mut app, "<Esc>/");
        assert_eq!(app.mode(), Mode::Search);
    }

    #[test]
    fn a_filter_that_cannot_be_understood_stays_in_the_prompt() {
        let mut app = app();
        press(&mut app, "fcyclomatic ><CR>");
        assert_eq!(app.mode(), Mode::Filter);
        assert_eq!(
            app.prompt().and_then(|prompt| prompt.error.as_deref()),
            Some("Invalid filter: expected a value after >")
        );
        press(&mut app, "<BS>> 1<CR>");
        assert_eq!(app.mode(), Mode::Normal);
        assert_eq!(
            app.lens()
                .filter
                .as_ref()
                .map(|filter| filter.text.as_str()),
            Some("cyclomatic > 1")
        );
        press(&mut app, "F");
        assert!(app.lens().filter.is_none());
    }

    #[test]
    fn commands_complete_and_unknown_ones_say_so() {
        let mut app = app();
        press(&mut app, ":exp<Tab>");
        assert_eq!(
            app.prompt().map(|prompt| prompt.text.as_str()),
            Some("export")
        );
        press(&mut app, "<Esc>:nonsense<CR>");
        assert_eq!(app.message(), Some("Unknown command: nonsense"));
        press(&mut app, ":init<CR>");
        assert!(app.take_effects().contains(&Effect::Init));
    }

    #[test]
    fn the_space_menu_runs_a_key_or_closes() {
        let mut app = app();
        press(&mut app, "<Space>");
        assert_eq!(app.popup_name(), Some("space menu"));
        assert_eq!(
            app.popup_keys().into_iter().collect::<String>(),
            "?adefmopqrstx"
        );
        press(&mut app, "z");
        assert_eq!(app.popup_name(), None);
        assert_eq!(app.view(), View::Overview);
        press(&mut app, "<Space><Space>");
        assert_eq!(app.popup_name(), None);
    }

    #[test]
    fn a_prefix_lists_what_can_follow_only_after_a_pause() {
        let mut app = app();
        press(&mut app, "g");
        assert_eq!(app.popup_name(), None);
        app.update(Input::Tick(HINT_DELAY_MS - 1));
        assert_eq!(app.popup_name(), None);
        app.update(Input::Tick(HINT_DELAY_MS + 100));
        assert_eq!(app.popup_name(), Some("key hints"));
        assert_eq!(app.popup_keys().into_iter().collect::<String>(), "Tcdfgrt");
        press(&mut app, "t");
        assert_eq!(app.popup_name(), None);
        assert_eq!(app.view(), View::Map);
    }

    #[test]
    fn q_closes_what_is_open_and_never_quits() {
        let mut app = app();
        press(&mut app, "q");
        assert!(app.running());
        press(&mut app, "K");
        assert_eq!(app.inspector(), Some(Section::Metrics));
        press(&mut app, "Hq");
        assert_eq!(app.inspector(), None);
        press(&mut app, ":q<CR>");
        assert!(!app.running());
        assert!(app.take_effects().contains(&Effect::Quit));
    }

    #[test]
    fn the_theme_follows_the_session_the_setting_and_the_terminal() {
        let mut app = App::new(Startup {
            background: Some(ThemeChoice::Light),
            ..startup(120, 36)
        });
        assert_eq!(app.theme(), Theme::Light);
        press(&mut app, ":set bg=dark<CR>");
        assert_eq!(app.theme(), Theme::Dark);
        press(&mut app, ":set bg=pink<CR>");
        assert_eq!(app.message(), Some("Unknown theme: pink"));
        assert_eq!(app.theme(), Theme::Dark);
        let plain = App::new(Startup {
            no_color: true,
            theme: ThemeChoice::Light,
            ..startup(120, 36)
        });
        assert_eq!(plain.theme(), Theme::NoColor);
    }

    #[test]
    fn a_small_terminal_says_so_until_it_grows() {
        let mut app = App::new(startup(79, 24));
        assert!(app.too_small());
        assert_eq!(app.panes(), Vec::<Pane>::new());
        let screen = screen_text(&render(&mut app));
        assert!(screen.contains("The terminal is too small. Cyclops needs 80 x 24."));
        app.update(Input::Resize {
            columns: 120,
            rows: 36,
        });
        assert_eq!(app.panes(), vec![Pane::Metrics, Pane::Ranking]);
    }

    #[test]
    fn the_inspector_covers_a_narrow_view() {
        let mut app = App::new(startup(99, 30));
        press(&mut app, "<Space>eK");
        assert_eq!(app.panes(), vec![Pane::Inspector]);
        app.update(Input::Resize {
            columns: 100,
            rows: 30,
        });
        assert_eq!(app.panes(), vec![Pane::Structure, Pane::Inspector]);
    }

    #[test]
    fn the_picker_narrows_by_name_or_label() {
        assert_eq!(picked("inst"), vec![MetricId::Instability]);
        assert_eq!(picked("cycle"), vec![MetricId::CycleSize]);
        assert_eq!(picked("").len(), MetricId::ALL.len());
    }

    fn node(id: &str) -> Node {
        Node {
            id: SymbolId::new(id),
            kind: SymbolKind::Module,
            overlay: None,
            alerted: false,
            in_cycle: false,
            stereotype: None,
            fields: Vec::new(),
            methods: Vec::new(),
        }
    }

    fn edge(from: &str, to: &str) -> Edge {
        Edge {
            from: SymbolId::new(from),
            to: SymbolId::new(to),
            kind: RelationKind::Imports,
            confidence: Confidence::Exact,
            in_cycle: false,
            from_members: Vec::new(),
            to_members: Vec::new(),
        }
    }

    fn columns(placed: &[Placed]) -> Vec<(&str, usize)> {
        placed
            .iter()
            .map(|placed| (placed.id.as_str(), placed.column))
            .collect()
    }

    #[test]
    fn dependents_go_left_of_the_focus_and_dependencies_right() {
        let view = DiagramView {
            nodes: vec![node("a"), node("b"), node("c")],
            edges: vec![edge("a", "b"), edge("b", "c")],
        };
        let placed = layout(&view, Some(&SymbolId::new("b")));
        assert_eq!(columns(&placed), vec![("a", 0), ("b", 1), ("c", 2)]);
    }

    #[test]
    fn without_a_focus_each_node_sits_right_of_what_depends_on_it() {
        let view = DiagramView {
            nodes: vec![node("a"), node("b"), node("c"), node("d")],
            edges: vec![edge("a", "b"), edge("b", "c")],
        };
        let placed = layout(&view, None);
        assert_eq!(
            columns(&placed),
            vec![("a", 0), ("d", 0), ("b", 1), ("c", 2)]
        );
    }

    #[test]
    fn precise_analysis_asks_first_and_runs_on_yes() {
        let mut app = app();
        app.take_effects();
        press(&mut app, "<Space>p");
        assert_eq!(app.take_effects(), vec![Effect::CheckPrecise]);
        app.precise_checked(Ok(()));
        assert_eq!(app.mode(), Mode::Confirm);
        assert_eq!(app.popup_name(), Some("confirm precise"));
        press(&mut app, "n");
        assert_eq!(app.popup_name(), None);
        assert_eq!(app.take_effects(), Vec::<Effect>::new());
        app.precise_checked(Ok(()));
        press(&mut app, "y");
        assert_eq!(
            app.take_effects(),
            vec![Effect::Analyze {
                tests: None,
                precise: true
            }]
        );
        app.precise_checked(Err("rust-analyzer is not installed.".to_owned()));
        assert_eq!(app.message(), Some("rust-analyzer is not installed."));
    }

    #[test]
    fn an_analysis_that_runs_shows_where_it_is() {
        let mut app = App::new(startup(120, 36));
        assert_eq!(app.mode(), Mode::Analyzing);
        assert_eq!(app.pose(), Pose::Scanning);
        app.heard(Heard::Step(Step::Extract));
        app.heard(Heard::Files(2, 6));
        let running = app.running_analysis().unwrap();
        assert_eq!(running.files, (2, 6));
        assert!(running.past(Step::Check));
        assert!(!running.past(Step::Extract));
        app.analyzed(Err("bad settings".to_owned()));
        assert_eq!(app.mode(), Mode::Normal);
        assert_eq!(app.failure(), Some("bad settings"));
    }

    #[test]
    fn copying_takes_the_filter_or_the_whole_search() {
        let mut app = app();
        press(&mut app, "fkind = method<CR>yf");
        assert_eq!(app.copied(), Some("kind = method"));
        press(&mut app, "yc");
        assert_eq!(
            app.copied(),
            Some("cyclops query 'kind = method' --sort 'cyclomatic desc'")
        );
        assert!(
            app.take_effects()
                .contains(&Effect::Copy("kind = method".to_owned()))
        );
    }

    #[test]
    fn sorting_by_the_same_column_turns_the_order_around() {
        let mut app = app();
        press(&mut app, "<Space>f");
        assert_eq!(app.sorted_by(), (MetricId::Cyclomatic, false));
        press(&mut app, "slines<CR>");
        assert_eq!(app.sorted_by(), (MetricId::Lines, false));
        assert_eq!(app.lens().metric, MetricId::Lines);
        press(&mut app, "slines<CR>");
        assert_eq!(app.sorted_by(), (MetricId::Lines, true));
    }

    #[test]
    fn without_an_editor_opening_one_says_how() {
        let mut app = app();
        press(&mut app, "gf");
        assert_eq!(app.message(), Some("Set $EDITOR to open files."));
        assert_eq!(app.opened(), None);
    }

    #[test]
    fn filters_come_back_with_up() {
        let mut app = app();
        press(&mut app, "fkind = struct<CR>F");
        assert!(app.lens().filter.is_none());
        app.update(Input::Key(Key::Char('f')));
        app.update(Input::Key(Key::Up));
        app.update(Input::Key(Key::Enter));
        assert_eq!(
            app.lens()
                .filter
                .as_ref()
                .map(|filter| filter.text.as_str()),
            Some("kind = struct")
        );
    }

    #[test]
    fn moving_stays_within_the_list() {
        assert_eq!(moved(0, -3, 5), 0);
        assert_eq!(moved(1, 9, 5), 4);
        assert_eq!(moved(3, 0, 0), 0);
    }
}

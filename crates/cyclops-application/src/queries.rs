//! What every interface can ask of an analysis: filters, alerts, ranking, impact and
//! explanations. See the pages in `specs/explore/`.

use crate::analysis::ProjectAnalysis;
use crate::analyzers::cohesion_groups;
use crate::graph::ModuleGraph;
use cyclops_domain::alert::AlertRule;
use cyclops_domain::facts::{BodyFacts, DecisionKind};
use cyclops_domain::filter::{Candidate, Comparison, Filter};
use cyclops_domain::language::LanguageId;
use cyclops_domain::metric::{MetricId, Scope, Subject};
use cyclops_domain::path::RelativePath;
use cyclops_domain::relation::{Confidence, RelationKind};
use cyclops_domain::settings::Severity;
use cyclops_domain::symbol::{CodeRole, Symbol, SymbolId, SymbolKind};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

/// An alert that fired.
#[derive(Debug, Clone, PartialEq)]
pub struct Alert {
    pub severity: Severity,
    pub subject: Subject,
    pub metric: MetricId,
    pub value: f64,
    pub rule: Option<String>,
    /// The place of the rule that fired it, among the rules given.
    pub rule_index: usize,
}

/// Which way to sort results by a metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
    Ascending,
    Descending,
}

/// Where a symbol stands among its peers: `position` of `of`, 1 being the worst.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rank {
    pub position: usize,
    pub of: usize,
}

/// How a rule's metric is spread across its scope, one cell per step from the lowest
/// value to the highest, and where the rule's line falls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleSpread {
    /// How many values fall in each cell.
    pub counts: Vec<usize>,
    /// The cell the threshold falls in.
    pub line: usize,
    /// The cell of the worst value.
    pub worst: usize,
    /// How many values the rule leaves alone, and how many it fires on.
    pub under: usize,
    pub over: usize,
}

/// The symbols ranked first for a metric.
#[derive(Debug, Clone, PartialEq)]
pub struct Worst {
    pub subjects: Vec<Subject>,
    pub value: f64,
}

/// What depends on a symbol: each dependent, at its nearest distance, with how sure
/// the surest of its shortest ways there is.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Impact {
    /// The dependents at each distance, the first being distance 1.
    pub levels: Vec<Vec<(SymbolId, Confidence)>>,
}

impl Impact {
    /// Every dependent, at any distance.
    pub fn all(&self) -> impl Iterator<Item = &(SymbolId, Confidence)> {
        self.levels.iter().flatten()
    }
}

/// How far an impact reaches: the modules and packages its dependents are in, of how many.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reach {
    pub modules: usize,
    pub of_modules: usize,
    pub packages: usize,
    pub of_packages: usize,
}

/// What a function's complexity is made of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Explanation {
    /// Each line with points, and how many.
    pub points: Vec<(u32, u32)>,
    /// How many decision points there are, which is what cyclomatic complexity counts.
    pub decisions: u32,
    /// How many of the points come from nesting.
    pub nesting: u32,
    /// The decision points, counted by kind.
    pub kinds: Vec<(&'static str, u32)>,
    /// The points, added up by kind.
    pub points_by_kind: Vec<(&'static str, u32)>,
    /// Where nesting is deepest, and the structures that lead there.
    pub deepest: Option<(u32, Vec<&'static str>)>,
}

/// A group of methods that has nothing to do with the others, and the fields it uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodGroup {
    pub methods: Vec<String>,
    pub fields: Vec<String>,
}

/// The peers a subject is ranked among.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Peers {
    Function,
    Type,
    Module,
    Package,
    File,
}

fn peers_of_kind(kind: SymbolKind) -> Option<Peers> {
    match kind {
        SymbolKind::Function | SymbolKind::Method => Some(Peers::Function),
        SymbolKind::Package => Some(Peers::Package),
        SymbolKind::Module => Some(Peers::Module),
        kind if kind.is_type() => Some(Peers::Type),
        _ => None,
    }
}

const fn peers_of_scope(scope: Scope) -> Peers {
    match scope {
        Scope::Function => Peers::Function,
        Scope::Type => Peers::Type,
        Scope::Module => Peers::Module,
        Scope::File => Peers::File,
        Scope::Project => Peers::Package,
    }
}

/// A symbol, as a filter sees it.
struct Seen<'a> {
    analysis: &'a ProjectAnalysis,
    symbol: &'a Symbol,
    languages: &'a HashMap<&'a RelativePath, LanguageId>,
    cycles: &'a HashSet<SymbolId>,
}

impl Candidate for Seen<'_> {
    fn metric(&self, metric: MetricId) -> Option<f64> {
        self.analysis
            .value(&Subject::Symbol(self.symbol.id.clone()), metric)
    }

    fn kind(&self) -> SymbolKind {
        self.symbol.kind
    }

    fn role(&self) -> CodeRole {
        self.symbol.role
    }

    fn language(&self) -> Option<LanguageId> {
        let file = &self.symbol.location.as_ref()?.file;
        self.languages.get(file).copied()
    }

    fn path(&self) -> Option<&str> {
        self.symbol
            .location
            .as_ref()
            .map(|location| location.file.as_str())
    }

    fn in_cycle(&self) -> Option<bool> {
        (self.symbol.kind == SymbolKind::Module).then(|| self.cycles.contains(&self.symbol.id))
    }
}

fn sorted_by_value(mut values: Vec<(Subject, f64)>) -> Vec<(Subject, f64)> {
    values.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    values
}

impl ProjectAnalysis {
    fn languages(&self) -> HashMap<&RelativePath, LanguageId> {
        self.files()
            .iter()
            .map(|file| (&file.path, file.language))
            .collect()
    }

    fn modules_in_cycles(&self) -> HashSet<SymbolId> {
        ModuleGraph::of(&self.view())
            .cycles()
            .into_iter()
            .flatten()
            .collect()
    }

    /// The symbols a filter matches, in order of id unless `sort` says otherwise. Ties
    /// stay in order of id, and symbols without a value go last.
    #[must_use]
    pub fn search(
        &self,
        filter: &Filter,
        sort: Option<(MetricId, Order)>,
        limit: Option<usize>,
    ) -> Vec<&Symbol> {
        let languages = self.languages();
        let cycles = self.modules_in_cycles();
        let mut found: Vec<&Symbol> = self
            .view()
            .symbols()
            .filter(|symbol| {
                filter.matches(&Seen {
                    analysis: self,
                    symbol,
                    languages: &languages,
                    cycles: &cycles,
                })
            })
            .collect();
        if let Some((metric, order)) = sort {
            found.sort_by(|a, b| {
                let value =
                    |symbol: &Symbol| self.value(&Subject::Symbol(symbol.id.clone()), metric);
                match (value(a), value(b)) {
                    (Some(x), Some(y)) => match order {
                        Order::Ascending => x.total_cmp(&y),
                        Order::Descending => y.total_cmp(&x),
                    },
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                }
            });
        }
        if let Some(limit) = limit {
            found.truncate(limit);
        }
        found
    }

    /// Whether a view of the hierarchy keeps a symbol: it matches, or something inside
    /// it does.
    #[must_use]
    pub fn kept(&self, filter: &Filter, id: &SymbolId) -> bool {
        let view = self.view();
        self.search(filter, None, None)
            .iter()
            .any(|found| view.is_within(&found.id, id))
    }

    /// Whether a subject is of a scope, as alert rules mean it.
    fn of_scope(&self, subject: &Subject, scope: Scope) -> bool {
        match subject {
            Subject::File(_) => scope == Scope::File,
            Subject::Symbol(id) => self.view().symbol(id).is_some_and(|symbol| match scope {
                Scope::Function => matches!(symbol.kind, SymbolKind::Function | SymbolKind::Method),
                Scope::Type => symbol.kind.is_type(),
                Scope::Module => symbol.kind == SymbolKind::Module,
                Scope::File | Scope::Project => false,
            }),
        }
    }

    fn file_of(&self, subject: &Subject) -> Option<RelativePath> {
        match subject {
            Subject::File(path) => Some(path.clone()),
            Subject::Symbol(id) => self
                .view()
                .symbol(id)
                .and_then(|symbol| symbol.location.as_ref())
                .map(|location| location.file.clone()),
        }
    }

    /// The alerts the rules fire, in order of severity and then subject.
    #[must_use]
    pub fn fire(&self, rules: &[AlertRule]) -> Vec<Alert> {
        let mut alerts = Vec::new();
        for (rule_index, rule) in rules.iter().enumerate().filter(|(_, rule)| rule.enabled) {
            for (subject, metric, value) in self.measurements() {
                if metric != rule.metric
                    || !self.of_scope(subject, rule.scope)
                    || !rule.fires(value)
                {
                    continue;
                }
                let in_paths = rule.paths.is_empty()
                    || self.file_of(subject).is_some_and(|file| {
                        rule.paths.iter().any(|glob| glob.matches(file.as_str()))
                    });
                if in_paths {
                    alerts.push(Alert {
                        severity: rule.severity,
                        subject: subject.clone(),
                        metric,
                        value,
                        rule: rule.name.clone(),
                        rule_index,
                    });
                }
            }
        }
        alerts.sort_by(|a, b| {
            b.severity
                .cmp(&a.severity)
                .then_with(|| a.subject.cmp(&b.subject))
        });
        alerts
    }

    fn peers_of(&self, subject: &Subject) -> Option<Peers> {
        match subject {
            Subject::File(_) => Some(Peers::File),
            Subject::Symbol(id) => self
                .view()
                .symbol(id)
                .and_then(|symbol| peers_of_kind(symbol.kind)),
        }
    }

    /// Every peer with a value for a metric, the worst first.
    fn ranked(&self, peers: Peers, metric: MetricId) -> Vec<(Subject, f64)> {
        let values = self
            .measurements()
            .filter(|(subject, measured, _)| {
                *measured == metric && self.peers_of(subject) == Some(peers)
            })
            .map(|(subject, _, value)| (subject.clone(), value))
            .collect();
        sorted_by_value(values)
    }

    /// Where a subject stands among the peers that have a value for a metric. Ties share
    /// a rank, and the next rank is skipped.
    #[must_use]
    pub fn rank(&self, subject: &Subject, metric: MetricId) -> Option<Rank> {
        let value = self.value(subject, metric)?;
        let ranked = self.ranked(self.peers_of(subject)?, metric);
        let worse = ranked
            .iter()
            .filter(|(_, other)| *other > value + 0.005)
            .count();
        Some(Rank {
            position: worse + 1,
            of: ranked.len(),
        })
    }

    /// The values of a metric among the peers of a scope, with the lowest and highest.
    fn peer_values(&self, scope: Scope, metric: MetricId) -> Option<(Vec<f64>, f64, f64)> {
        let values: Vec<f64> = self
            .ranked(peers_of_scope(scope), metric)
            .into_iter()
            .map(|(_, value)| value)
            .collect();
        let lowest = values.iter().copied().reduce(f64::min)?;
        let highest = values.iter().copied().reduce(f64::max)?;
        Some((values, lowest, highest))
    }

    /// How a metric is spread across the peers of a scope: eight equal ranges from the
    /// lowest value to the highest, with how many fall in each.
    #[must_use]
    pub fn spread(&self, scope: Scope, metric: MetricId) -> Option<[usize; 8]> {
        let (values, lowest, highest) = self.peer_values(scope, metric)?;
        let mut counts = [0; 8];
        for value in values {
            if let Some(count) = counts.get_mut(spread_range_of(value, lowest, highest)) {
                *count += 1;
            }
        }
        Some(counts)
    }

    /// Which of the eight ranges of [`Self::spread`] a value falls in.
    #[must_use]
    pub fn spread_range(&self, scope: Scope, metric: MetricId, value: f64) -> Option<usize> {
        let (_, lowest, highest) = self.peer_values(scope, metric)?;
        Some(spread_range_of(value, lowest, highest))
    }

    /// How a rule's metric is spread across every subject of its scope, in at most `cells`
    /// cells. A count gets a cell for each whole step, when there are few enough.
    #[must_use]
    pub fn rule_spread(&self, rule: &AlertRule, cells: usize) -> Option<RuleSpread> {
        let values: Vec<f64> = self
            .measurements()
            .filter(|(subject, metric, _)| {
                *metric == rule.metric && self.of_scope(subject, rule.scope)
            })
            .map(|(_, _, value)| value)
            .collect();
        let lowest = values.iter().copied().reduce(f64::min)?;
        let highest = values.iter().copied().reduce(f64::max)?;
        let span = highest - lowest;
        let float = |index: usize| f64::from(u32::try_from(index).unwrap_or(u32::MAX));
        // A cell for each whole step, such as each count from 1 to 31, if they fit.
        let whole = (span.round() - span).abs() < f64::EPSILON && !rule.metric.descriptor().ratio;
        let count = if whole {
            (1..=cells)
                .find(|count| float(*count) > span)
                .unwrap_or(cells)
        } else {
            cells
        }
        .max(1);
        let last = float(count - 1);
        let cell = |value: f64| {
            let share = if span > 0.0 {
                (value - lowest) / span
            } else {
                0.0
            };
            let at = (share.clamp(0.0, 1.0) * last).round();
            (0..count)
                .rev()
                .find(|index| float(*index) <= at)
                .unwrap_or(0)
        };
        let mut counts = vec![0; count];
        let (mut under, mut over) = (0, 0);
        for value in &values {
            if let Some(slot) = counts.get_mut(cell(*value)) {
                *slot += 1;
            }
            if rule.fires(*value) {
                over += 1;
            } else {
                under += 1;
            }
        }
        Some(RuleSpread {
            counts,
            line: cell(rule.threshold),
            worst: cell(highest),
            under,
            over,
        })
    }

    /// The subject a rule would fire on next if its threshold moved: of those it leaves
    /// alone, the one whose value is nearest to firing.
    #[must_use]
    pub fn next_closest(&self, rule: &AlertRule) -> Option<(Subject, f64)> {
        let higher_fires = matches!(
            rule.comparison,
            Comparison::Greater | Comparison::GreaterOrEqual | Comparison::NotEqual
        );
        self.measurements()
            .filter(|(subject, metric, value)| {
                *metric == rule.metric && self.of_scope(subject, rule.scope) && !rule.fires(*value)
            })
            .map(|(subject, _, value)| (subject.clone(), value))
            .max_by(|(a, x), (b, y)| {
                let order = if higher_fires {
                    x.total_cmp(y)
                } else {
                    y.total_cmp(x)
                };
                order.then_with(|| b.cmp(a))
            })
    }

    /// The worst subjects for a metric where it is measured, worst first, as the
    /// Overview ranks them. A filter keeps only the symbols it matches.
    #[must_use]
    pub fn ranking(
        &self,
        metric: MetricId,
        filter: Option<&Filter>,
        limit: usize,
    ) -> Vec<(Subject, f64)> {
        let matched: Option<HashSet<SymbolId>> = filter.map(|filter| {
            self.search(filter, None, None)
                .iter()
                .map(|symbol| symbol.id.clone())
                .collect()
        });
        self.ranked(peers_of_scope(metric.descriptor().measured_on()), metric)
            .into_iter()
            .filter(|(subject, _)| match (subject, &matched) {
                (_, None) => true,
                (Subject::Symbol(id), Some(matched)) => matched.contains(id),
                (Subject::File(_), Some(_)) => false,
            })
            .take(limit)
            .collect()
    }

    /// The subjects ranked first for a metric, where the metric is measured.
    #[must_use]
    pub fn worst(&self, metric: MetricId) -> Option<Worst> {
        let ranked = self.ranked(peers_of_scope(metric.descriptor().measured_on()), metric);
        let (_, value) = ranked.first()?.clone();
        let mut subjects: Vec<Subject> = ranked
            .into_iter()
            .take_while(|(_, other)| (other - value).abs() < 0.005)
            .map(|(subject, _)| subject)
            .collect();
        subjects.sort();
        Some(Worst { subjects, value })
    }

    /// The modules and packages an impact reaches. Only modules written in a file count,
    /// so that a folder and the file that is its module count once.
    #[must_use]
    pub fn impact_reach(&self, impact: &Impact) -> Reach {
        let view = self.view();
        let mut modules = BTreeSet::new();
        let mut packages = BTreeSet::new();
        for (id, _) in impact.all() {
            if let Some(module) = view.module_of(id) {
                modules.insert(module.id.clone());
            }
            let mut top = view.symbol(id);
            while let Some(parent) = top
                .and_then(|symbol| symbol.parent.as_ref())
                .and_then(|parent| view.symbol(parent))
            {
                top = Some(parent);
            }
            if let Some(top) = top.filter(|top| top.kind == SymbolKind::Package) {
                packages.insert(top.id.clone());
            }
        }
        let count = |kind: SymbolKind, located: bool| {
            view.symbols()
                .filter(|symbol| symbol.kind == kind && (!located || symbol.location.is_some()))
                .count()
        };
        Reach {
            modules: modules.len(),
            of_modules: count(SymbolKind::Module, true),
            packages: packages.len(),
            of_packages: count(SymbolKind::Package, false),
        }
    }

    /// What depends on a symbol, following the relations its level depends through.
    #[must_use]
    pub fn impact(&self, id: &SymbolId) -> Impact {
        let view = self.view();
        let Some(symbol) = view.symbol(id) else {
            return Impact::default();
        };
        let kinds: &[RelationKind] = match symbol.kind {
            SymbolKind::Module | SymbolKind::Package => &[RelationKind::Imports],
            SymbolKind::Function | SymbolKind::Method => &[RelationKind::Calls],
            _ => &[
                RelationKind::Uses,
                RelationKind::Composes,
                RelationKind::Implements,
                RelationKind::Inherits,
            ],
        };
        let mut callers: HashMap<&SymbolId, Vec<(&SymbolId, Confidence)>> = HashMap::new();
        for relation in view
            .relations()
            .filter(|relation| kinds.contains(&relation.kind))
        {
            callers
                .entry(&relation.to)
                .or_default()
                .push((&relation.from, relation.confidence));
        }
        let mut seen: HashSet<&SymbolId> = HashSet::from([id]);
        let mut frontier: BTreeMap<&SymbolId, Confidence> =
            BTreeMap::from([(id, Confidence::Exact)]);
        let mut levels = Vec::new();
        while !frontier.is_empty() {
            let mut next: BTreeMap<&SymbolId, Confidence> = BTreeMap::new();
            for (target, sure) in &frontier {
                for (dependent, confidence) in callers.get(target).into_iter().flatten() {
                    if seen.contains(dependent) {
                        continue;
                    }
                    let weakest = (*sure).max(*confidence);
                    let slot = next.entry(dependent).or_insert(weakest);
                    *slot = (*slot).min(weakest);
                }
            }
            seen.extend(next.keys().copied());
            if !next.is_empty() {
                levels.push(
                    next.iter()
                        .map(|(id, sure)| ((*id).clone(), *sure))
                        .collect(),
                );
            }
            frontier = next;
        }
        Impact { levels }
    }

    /// What a function's complexity is made of.
    #[must_use]
    pub fn explain(&self, id: &SymbolId) -> Option<Explanation> {
        let body: &BodyFacts = self.view().symbol(id)?.body.as_ref()?;
        let mut points: BTreeMap<u32, u32> = BTreeMap::new();
        let mut kinds: BTreeMap<&'static str, u32> = BTreeMap::new();
        let mut by_kind: BTreeMap<&'static str, u32> = BTreeMap::new();
        let (mut decisions, mut nesting) = (0, 0);
        for point in &body.decisions {
            let earned = point.kind.points(point.nesting);
            if earned > 0 {
                *points.entry(point.line).or_insert(0) += earned;
                *by_kind.entry(point.kind.name()).or_insert(0) += earned;
                nesting += earned - point.kind.points(0);
            }
            let paths = point.kind.paths();
            if paths > 0 {
                decisions += paths;
                *kinds.entry(point.kind.name()).or_insert(0) += paths;
            }
        }
        Some(Explanation {
            points: points.into_iter().collect(),
            decisions,
            nesting,
            kinds: kinds.into_iter().collect(),
            points_by_kind: by_kind.into_iter().collect(),
            deepest: body.deepest.as_ref().map(|deepest| {
                (
                    deepest.line,
                    deepest
                        .path
                        .iter()
                        .map(|kind| DecisionKind::name(*kind))
                        .collect(),
                )
            }),
        })
    }

    /// Where a function stands among the functions of its file, for a metric.
    #[must_use]
    pub fn in_file(&self, id: &SymbolId, metric: MetricId) -> Option<Rank> {
        let view = self.view();
        let file = view.symbol(id)?.location.as_ref()?.file.clone();
        let value = self.value(&Subject::Symbol(id.clone()), metric)?;
        let values: Vec<f64> = view
            .symbols()
            .filter(|symbol| matches!(symbol.kind, SymbolKind::Function | SymbolKind::Method))
            .filter(|symbol| {
                symbol
                    .location
                    .as_ref()
                    .is_some_and(|location| location.file == file)
            })
            .filter_map(|symbol| self.value(&Subject::Symbol(symbol.id.clone()), metric))
            .collect();
        let worse = values
            .iter()
            .filter(|other| **other > value + 0.005)
            .count();
        Some(Rank {
            position: worse + 1,
            of: values.len(),
        })
    }

    /// The groups of methods of a type that have nothing to do with each other, which is
    /// what cohesion counts, or nothing for a type that has no cohesion.
    #[must_use]
    pub fn method_groups(&self, id: &SymbolId) -> Option<Vec<MethodGroup>> {
        let view = self.view();
        let children: Vec<&Symbol> = view.children(id).collect();
        let fields: Vec<&str> = children
            .iter()
            .filter(|child| child.kind == SymbolKind::Field)
            .map(|child| child.name.as_str())
            .collect();
        let empty = BodyFacts::default();
        let methods: Vec<(&str, &BodyFacts)> = children
            .iter()
            .filter(|child| {
                child.kind == SymbolKind::Method
                    && child.member.instance
                    && !child.member.for_interface
            })
            .map(|child| (child.name.as_str(), child.body.as_ref().unwrap_or(&empty)))
            .collect();
        let groups = cohesion_groups(&fields, &methods)?;
        let mut described: Vec<MethodGroup> = groups
            .into_iter()
            .map(|members| {
                let mut names: Vec<String> = Vec::new();
                let mut used: BTreeSet<String> = BTreeSet::new();
                for index in members {
                    if let Some((name, body)) = methods.get(index) {
                        names.push((*name).to_owned());
                        used.extend(
                            body.fields_used
                                .iter()
                                .filter(|field| fields.contains(&field.as_str()))
                                .cloned(),
                        );
                    }
                }
                names.sort();
                MethodGroup {
                    methods: names,
                    fields: used.into_iter().collect(),
                }
            })
            .collect();
        described.sort_by(|a, b| a.methods.cmp(&b.methods));
        Some(described)
    }

    /// A way around the cycle of a module, from it back to it, with each import on the
    /// way and how sure it is.
    #[must_use]
    pub fn cycle_path(&self, id: &SymbolId) -> Option<Vec<(SymbolId, SymbolId, Confidence)>> {
        let view = self.view();
        let graph = ModuleGraph::of(&view);
        let cycle: BTreeSet<SymbolId> = graph
            .cycles()
            .into_iter()
            .find(|cycle| cycle.contains(id))?
            .into_iter()
            .collect();
        let mut came_from: HashMap<SymbolId, SymbolId> = HashMap::new();
        let mut queue = VecDeque::from([id.clone()]);
        let mut back = None;
        while let Some(current) = queue.pop_front() {
            for next in graph
                .depends_on(&current)
                .into_iter()
                .filter(|next| cycle.contains(*next))
            {
                if next == id {
                    back = Some(current.clone());
                    break;
                }
                if !came_from.contains_key(next) {
                    came_from.insert(next.clone(), current.clone());
                    queue.push_back(next.clone());
                }
            }
            if back.is_some() {
                break;
            }
        }
        let mut path = vec![id.clone()];
        let mut current = back?;
        while &current != id {
            path.push(current.clone());
            current = came_from.get(&current)?.clone();
        }
        path.push(id.clone());
        path.reverse();
        Some(
            path.windows(2)
                .filter_map(|pair| {
                    let (from, to) = (pair.first()?, pair.get(1)?);
                    let confidence = view
                        .relations()
                        .filter(|relation| {
                            relation.kind == RelationKind::Imports
                                && &relation.from == from
                                && &relation.to == to
                        })
                        .map(|relation| relation.confidence)
                        .min()?;
                    Some((from.clone(), to.clone(), confidence))
                })
                .collect(),
        )
    }
}

/// The range of eight, from `lowest` to `highest`, that a value falls in.
fn spread_range_of(value: f64, lowest: f64, highest: f64) -> usize {
    let share = if highest > lowest {
        (value - lowest) / (highest - lowest)
    } else {
        0.0
    };
    (0_u8..8)
        .rev()
        .find(|range| share * 8.0 >= f64::from(*range))
        .map_or(0, usize::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{Extras, ProjectAnalysis};
    use cyclops_domain::metric::Measurement;
    use cyclops_domain::model::{ProjectModel, TestCode};
    use cyclops_domain::relation::Relation;

    fn symbol(id: &str, kind: SymbolKind) -> Symbol {
        Symbol::container(SymbolId::new(id), None, id, kind, CodeRole::Production)
    }

    fn calls(from: &str, to: &str, confidence: Confidence) -> Relation {
        Relation {
            from: SymbolId::new(from),
            to: SymbolId::new(to),
            kind: RelationKind::Calls,
            confidence,
        }
    }

    fn analysis(
        symbols: Vec<Symbol>,
        relations: Vec<Relation>,
        measurements: Vec<Measurement>,
    ) -> ProjectAnalysis {
        ProjectAnalysis::new(
            ProjectModel::new(symbols, relations),
            Vec::new(),
            measurements,
            Vec::new(),
            TestCode::Excluded,
            0,
            Extras::default(),
        )
    }

    #[test]
    fn a_dependent_is_as_sure_as_its_surest_shortest_way() {
        let functions = ["target", "a", "b", "c"].map(|id| symbol(id, SymbolKind::Function));
        let project = analysis(
            functions.to_vec(),
            vec![
                calls("a", "target", Confidence::Ambiguous),
                calls("b", "target", Confidence::Exact),
                calls("c", "a", Confidence::Exact),
                calls("c", "b", Confidence::Exact),
            ],
            Vec::new(),
        );
        let impact = project.impact(&SymbolId::new("target"));
        let level = |index: usize| impact.levels.get(index).cloned().unwrap_or_default();
        assert_eq!(
            level(0),
            [
                (SymbolId::new("a"), Confidence::Ambiguous),
                (SymbolId::new("b"), Confidence::Exact)
            ]
        );
        assert_eq!(level(1), [(SymbolId::new("c"), Confidence::Exact)]);
    }

    #[test]
    fn a_loop_of_callers_ends() {
        let functions = ["a", "b"].map(|id| symbol(id, SymbolKind::Function));
        let project = analysis(
            functions.to_vec(),
            vec![
                calls("a", "b", Confidence::Exact),
                calls("b", "a", Confidence::Exact),
            ],
            Vec::new(),
        );
        let impact = project.impact(&SymbolId::new("a"));
        assert_eq!(impact.all().count(), 1);
    }

    #[test]
    fn values_that_are_all_alike_spread_into_the_first_range() {
        let functions = ["a", "b"].map(|id| symbol(id, SymbolKind::Function));
        let measured = ["a", "b"].map(|id| Measurement {
            metric: MetricId::Cyclomatic,
            subject: Subject::Symbol(SymbolId::new(id)),
            value: 4.0,
        });
        let project = analysis(functions.to_vec(), Vec::new(), measured.to_vec());
        assert_eq!(
            project.spread(Scope::Function, MetricId::Cyclomatic),
            Some([2, 0, 0, 0, 0, 0, 0, 0])
        );
        let rank = project.rank(&Subject::Symbol(SymbolId::new("b")), MetricId::Cyclomatic);
        assert_eq!(rank, Some(Rank { position: 1, of: 2 }));
    }

    #[test]
    fn the_next_closest_is_the_one_nearest_to_firing() {
        let functions = ["a", "b", "c"].map(|id| symbol(id, SymbolKind::Function));
        let measurements = [("a", 3.0), ("b", 2.0), ("c", 1.0)]
            .map(|(id, value)| Measurement {
                metric: MetricId::Cyclomatic,
                subject: Subject::Symbol(SymbolId::new(id)),
                value,
            })
            .to_vec();
        let project = analysis(functions.to_vec(), Vec::new(), measurements);
        let rule = |when: &str| {
            AlertRule::parse(&cyclops_domain::settings::AlertSource {
                fields: [
                    ("metric", "cyclomatic"),
                    ("scope", "function"),
                    ("when", when),
                    ("severity", "warning"),
                ]
                .iter()
                .map(|(name, value)| {
                    (
                        (*name).to_owned(),
                        cyclops_domain::settings::RawValue::Text((*value).to_owned()),
                    )
                })
                .collect(),
                origin: cyclops_domain::settings::Origin::ProjectFile,
                position: 0,
            })
            .unwrap()
        };
        let closest = |when: &str| {
            project
                .next_closest(&rule(when))
                .map(|(subject, _)| subject.to_string())
        };
        assert_eq!(closest("> 2").as_deref(), Some("b"));
        assert_eq!(closest("< 2").as_deref(), Some("b"));
        assert_eq!(closest("> 0"), None);
    }

    fn cyclomatic(values: &[(&str, f64)]) -> ProjectAnalysis {
        let functions = values
            .iter()
            .map(|(id, _)| symbol(id, SymbolKind::Function))
            .collect();
        let measurements = values
            .iter()
            .map(|(id, value)| Measurement {
                metric: MetricId::Cyclomatic,
                subject: Subject::Symbol(SymbolId::new(id)),
                value: *value,
            })
            .collect();
        analysis(functions, Vec::new(), measurements)
    }

    fn rule(when: &str, threshold: f64) -> AlertRule {
        AlertRule {
            name: None,
            metric: MetricId::Cyclomatic,
            scope: Scope::Function,
            comparison: Comparison::Greater,
            threshold,
            when: when.to_owned(),
            severity: Severity::Warning,
            paths: Vec::new(),
            enabled: true,
            origin: cyclops_domain::settings::Origin::ProjectFile,
            position: 0,
        }
    }

    #[test]
    fn a_rule_spreads_a_count_one_cell_for_each_value() {
        let project = cyclomatic(&[("a", 1.0), ("b", 1.0), ("c", 2.0), ("d", 5.0)]);
        let spread = project.rule_spread(&rule("> 2", 2.0), 60).unwrap();
        // From 1 to 5 is five cells, the line is on 2, and two values fire.
        assert_eq!(spread.counts, vec![2, 1, 0, 0, 1]);
        assert_eq!((spread.line, spread.worst), (1, 4));
        assert_eq!((spread.under, spread.over), (3, 1));
        let narrow = project.rule_spread(&rule("> 2", 2.0), 3).unwrap();
        assert_eq!(narrow.counts.len(), 3);
        assert_eq!(narrow.counts.iter().sum::<usize>(), 4);
    }

    #[test]
    fn a_value_falls_in_the_range_its_spread_counts_it_in() {
        let project = cyclomatic(&[("a", 1.0), ("b", 4.0), ("c", 9.0)]);
        let spread = project
            .spread(Scope::Function, MetricId::Cyclomatic)
            .unwrap();
        for value in [1.0, 4.0, 9.0] {
            let range = project
                .spread_range(Scope::Function, MetricId::Cyclomatic, value)
                .unwrap();
            assert!(spread.get(range).is_some_and(|count| *count > 0));
        }
        assert_eq!(
            project.spread_range(Scope::Function, MetricId::Cyclomatic, 9.0),
            Some(7)
        );
        assert_eq!(
            project.spread_range(Scope::Module, MetricId::Cyclomatic, 9.0),
            None
        );
    }

    #[test]
    fn an_impact_reaches_the_modules_and_packages_of_its_dependents() {
        let mut symbols = vec![
            symbol("shop", SymbolKind::Package),
            symbol("tools", SymbolKind::Package),
        ];
        for (id, parent, kind) in [
            ("shop::a", "shop", SymbolKind::Module),
            ("shop::b", "shop", SymbolKind::Module),
            ("tools::c", "tools", SymbolKind::Module),
            ("shop::a::target", "shop::a", SymbolKind::Function),
            ("shop::b::caller", "shop::b", SymbolKind::Function),
        ] {
            let mut one = symbol(id, kind);
            one.parent = Some(SymbolId::new(parent));
            if kind == SymbolKind::Module {
                one.location = Some(cyclops_domain::symbol::Location {
                    file: RelativePath::new(&format!("{id}.rs")),
                    span: cyclops_domain::symbol::Span {
                        start_line: 1,
                        end_line: 1,
                    },
                });
            }
            symbols.push(one);
        }
        let project = analysis(
            symbols,
            vec![calls(
                "shop::b::caller",
                "shop::a::target",
                Confidence::Exact,
            )],
            Vec::new(),
        );
        let impact = project.impact(&SymbolId::new("shop::a::target"));
        assert_eq!(
            project.impact_reach(&impact),
            Reach {
                modules: 1,
                of_modules: 3,
                packages: 1,
                of_packages: 2,
            }
        );
    }
}

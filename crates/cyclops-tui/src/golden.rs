//! Golden tests: the interface drawn at the design size, on the made-up project the design
//! draws, compared cell by cell with `specs/tui/design/screens/NAME.txt`.
//!
//! The made-up data lives here, in test code. Where the interface rightly differs from a
//! design picture, the cells are listed with the reason, and nowhere else may it differ.

use crate::app::{App, Heard};
use crate::canvas::screen_text;
use crate::input::{Gesture, Input, parse_gestures};
use crate::render;
use crate::state::{IndexerReport, Startup, ThemeChoice};
use cyclops_application::analysis::{AnalyzedFile, Extras, ProjectAnalysis};
use cyclops_application::ports::{Step, StepReport};
use cyclops_application::queries::Alert;
use cyclops_domain::alert::AlertRule;
use cyclops_domain::facts::LineCounts;
use cyclops_domain::filter::Comparison;
use cyclops_domain::language::LanguageId;
use cyclops_domain::metric::{Measurement, MetricId, Scope, Subject};
use cyclops_domain::model::{ProjectModel, TestCode};
use cyclops_domain::path::RelativePath;
use cyclops_domain::problem::Problem;
use cyclops_domain::relation::{Confidence, Relation, RelationKind};
use cyclops_domain::settings::{Origin, Severity};
use cyclops_domain::symbol::{CodeRole, Location, Span, Symbol, SymbolId, SymbolKind};
use std::fmt::Write as _;
use std::ops::Range;
use std::path::Path;

/// A made-up project, built up piece by piece.
#[derive(Default)]
struct Design {
    symbols: Vec<Symbol>,
    relations: Vec<Relation>,
    measurements: Vec<Measurement>,
    files: Vec<AnalyzedFile>,
    alerts: Vec<Alert>,
    rules: Vec<AlertRule>,
    problems: Vec<Problem>,
    steps: Vec<StepReport>,
}

impl Design {
    /// Adds a symbol whose parent is everything before its last `::`.
    fn symbol(&mut self, id: &str, kind: SymbolKind) {
        let name = id.rsplit("::").next().unwrap_or(id);
        self.named(id, name, kind);
    }

    /// Adds a symbol with a name of its own, such as a package `shop-api` for `shop_api`.
    fn named(&mut self, id: &str, name: &str, kind: SymbolKind) {
        let parent = id
            .rsplit_once("::")
            .map(|(parent, _)| SymbolId::new(parent));
        self.symbols.push(Symbol::container(
            SymbolId::new(id),
            parent,
            name,
            kind,
            CodeRole::Production,
        ));
    }

    /// Says where a symbol is written.
    fn place(&mut self, id: &str, file: &str, lines: (u32, u32)) {
        if let Some(symbol) = self
            .symbols
            .iter_mut()
            .find(|symbol| symbol.id.as_str() == id)
        {
            symbol.location = Some(Location {
                file: RelativePath::new(file),
                span: Span {
                    start_line: lines.0,
                    end_line: lines.1,
                },
            });
        }
    }

    fn measure(&mut self, id: &str, metric: MetricId, value: f64) {
        self.measurements.push(Measurement {
            metric,
            subject: Subject::Symbol(SymbolId::new(id)),
            value,
        });
    }

    /// Adds a symbol and its cyclomatic value.
    fn node(&mut self, id: &str, kind: SymbolKind, cyclomatic: f64) {
        self.symbol(id, kind);
        self.measure(id, MetricId::Cyclomatic, cyclomatic);
    }

    fn file(&mut self, path: &str, module: &str, language: LanguageId, code: u32) {
        self.files.push(AnalyzedFile {
            path: RelativePath::new(path),
            module: Some(SymbolId::new(module)),
            language,
            role: CodeRole::Production,
            lines: LineCounts {
                code,
                comment: 0,
                blank: 0,
            },
            test_lines: LineCounts::default(),
        });
    }

    fn relate(&mut self, from: &str, to: &str, kind: RelationKind, confidence: Confidence) {
        self.relations.push(Relation {
            from: SymbolId::new(from),
            to: SymbolId::new(to),
            kind,
            confidence,
        });
    }

    fn build(self) -> ProjectAnalysis {
        let extras = Extras {
            alerts: self.alerts,
            rules: self.rules,
            steps: self.steps,
            ..Extras::default()
        };
        ProjectAnalysis::from_parts(
            ProjectModel::new(self.symbols, self.relations),
            self.files,
            self.measurements,
            self.problems,
            TestCode::Excluded,
            3,
            extras,
        )
    }
}

/// The modules the design names, out of sight in parts that stay closed.
const NAMED_MODULES: [&str; 12] = [
    "shop::catalog::model",
    "shop::catalog::pricing",
    "shop_api::routes",
    "shop_api::admin",
    "shop_api::auth",
    "shop_jobs::export",
    "shop::notes::digest",
    "shop::inventory::audit",
    "storefront::cart",
    "storefront::checkout",
    "storefront::session",
    "storefront::orders",
];

/// The rest of the 188 modules, which the design does not name.
fn hidden_modules() -> Vec<String> {
    (0..164_usize)
        .map(|index| {
            let package = ["storefront", "shop_api", "shop_jobs"]
                .get(index % 3)
                .copied()
                .unwrap_or("storefront");
            format!("{package}::m{index:03}")
        })
        .collect()
}

/// The alert rules: a name, its metric, its severity, and how many alerts it fires.
const RULES: [(&str, MetricId, Severity, usize); 3] = [
    (
        "Very complex function",
        MetricId::Cyclomatic,
        Severity::Warning,
        23,
    ),
    ("Low cohesion in core", MetricId::Lcom, Severity::Error, 2),
    (
        "Far from the main line",
        MetricId::Distance,
        Severity::Warning,
        4,
    ),
];

/// The project the design draws: `shop`, with 188 modules, 612 types, 3,204 functions,
/// 842 files and 96,410 lines.
fn shop() -> Design {
    let mut design = Design::default();
    for (id, name, value) in [
        ("shop", "shop", 31.0),
        ("storefront", "storefront", 22.0),
        ("shop_api", "shop-api", 19.0),
        ("shop_jobs", "shop-jobs", 15.0),
    ] {
        design.named(id, name, SymbolKind::Package);
        design.measure(id, MetricId::Cyclomatic, value);
    }
    for (id, value) in [
        ("shop::billing", 31.0),
        ("shop::billing::invoice", 31.0),
        ("shop::billing::tax", 22.0),
        ("shop::billing::ledger", 12.0),
        ("shop::billing::rates", 5.0),
        ("shop::orders", 27.0),
        ("shop::orders::checkout", 27.0),
        ("shop::orders::service", 19.0),
        ("shop::orders::state", 9.0),
        ("shop::catalog", 24.0),
        ("shop::inventory", 13.0),
        ("shop::notes", 1.0),
    ] {
        design.node(id, SymbolKind::Module, value);
    }
    design.node("shop::billing::invoice::Invoice", SymbolKind::Struct, 31.0);
    design.node("shop::billing::invoice::LineItem", SymbolKind::Struct, 4.0);
    for (name, value) in [
        ("reconcile", 31.0),
        ("apply_credit", 14.0),
        ("finalize", 11.0),
        ("total", 6.0),
    ] {
        let id = format!("shop::billing::invoice::Invoice::{name}");
        design.node(&id, SymbolKind::Method, value);
    }
    // Something inside each part the design draws closed, so that it has an arrow.
    for parent in [
        "shop::billing::invoice::LineItem",
        "shop::billing::tax",
        "shop::billing::ledger",
        "shop::orders::checkout",
        "shop::orders::service",
        "shop::orders::state",
        "shop::inventory",
        "shop::notes",
    ] {
        design.symbol(&format!("{parent}::inner"), SymbolKind::Function);
    }
    for id in NAMED_MODULES
        .map(str::to_owned)
        .into_iter()
        .chain(hidden_modules())
    {
        design.node(&id, SymbolKind::Module, 2.0);
    }
    billing_metrics(&mut design);
    shop_relations(&mut design);
    highest(&mut design);
    shop_alerts(&mut design);
    shop_files(&mut design);
    design.rules = RULES
        .iter()
        .enumerate()
        .map(|(position, (name, metric, severity, _))| AlertRule {
            name: Some((*name).to_owned()),
            metric: *metric,
            scope: Scope::Function,
            comparison: Comparison::Greater,
            threshold: 1.0,
            when: "> 1".to_owned(),
            severity: *severity,
            paths: Vec::new(),
            enabled: true,
            origin: Origin::ProjectFile,
            position,
        })
        .collect();
    design.problems = vec![
        Problem::SyntaxError(RelativePath::new("storefront/src/legacy/old_cart.ts")),
        Problem::NotUtf8(RelativePath::new("src/billing/rates_table.rs")),
    ];
    design.steps = [
        (Step::Discover, 100, "842 files"),
        (Step::Check, 100, "3 changed"),
        (Step::Extract, 200, "3 parsed, 839 from cache"),
        (Step::Link, 1_400, "21,840 relations"),
        (Step::Enrich, 0, "skipped: no index yet"),
        (Step::History, 100, "2 new commits"),
        (Step::Analyze, 500, "all metrics"),
        (Step::Evaluate, 0, "29 alerts"),
    ]
    .into_iter()
    .map(|(step, millis, result)| StepReport {
        step,
        millis,
        result: result.to_owned(),
    })
    .collect();
    design
}

/// The indexers, as the design finds them: Rust ready, TypeScript not.
fn shop_indexers() -> Vec<IndexerReport> {
    vec![
        IndexerReport {
            label: "Rust".to_owned(),
            languages: vec![LanguageId::Rust],
            indexer: "rust-analyzer".to_owned(),
            ready: Ok(()),
        },
        IndexerReport {
            label: "TypeScript".to_owned(),
            languages: vec![LanguageId::TypeScript, LanguageId::JavaScript],
            indexer: "scip-typescript".to_owned(),
            ready: Err("The project's dependencies are not installed.".to_owned()),
        },
    ]
}

/// A metric of billing: its value, its rank among modules, and the highest module if not
/// billing.
type Standing<'a> = (MetricId, f64, usize, Option<(&'a str, f64)>);

/// The module the inspector shows, ranked among the others for each metric, and the
/// module the Overview names as the highest, if another.
fn billing_metrics(design: &mut Design) {
    let others = hidden_modules();
    let rows: [Standing<'_>; 8] = [
        (MetricId::Cognitive, 58.0, 1, None),
        (MetricId::Lines, 18_204.0, 1, None),
        (
            MetricId::Duplication,
            6.2,
            48,
            Some(("shop_jobs::export", 23.4)),
        ),
        (
            MetricId::Afferent,
            6.0,
            31,
            Some(("shop::catalog::model", 61.0)),
        ),
        (
            MetricId::Efferent,
            5.0,
            44,
            Some(("shop_api::routes", 38.0)),
        ),
        (
            MetricId::Instability,
            0.45,
            102,
            Some(("shop_api::routes", 0.94)),
        ),
        (MetricId::Distance, 0.41, 1, None),
        (MetricId::Hotspot, 1.0, 1, None),
    ];
    for (metric, value, rank, top) in rows {
        design.measure("shop::billing", metric, value);
        let mut above = rank - 1;
        if let Some((id, highest)) = top {
            design.measure(id, metric, highest);
            above -= 1;
        }
        // The modules ranked above it have half as much again, and the others a tenth.
        for (index, other) in others.iter().enumerate() {
            let share = if index < above { 1.5 } else { 0.1 };
            design.measure(other, metric, value * share);
        }
        for id in NAMED_MODULES {
            if top.is_none_or(|(top, _)| top != id) {
                design.measure(id, metric, value * 0.1);
            }
        }
    }
}

/// What the Overview names as the highest of each metric, and enough functions and types
/// besides to make up the design's counts.
fn highest(design: &mut Design) {
    let reconcile = "shop::billing::invoice::Invoice::reconcile";
    for (metric, value) in [
        (MetricId::Cognitive, 58.0),
        (MetricId::MaxNesting, 6.0),
        (MetricId::Lines, 168.0),
        (MetricId::Parameters, 3.0),
    ] {
        design.measure(reconcile, metric, value);
    }
    design.place(reconcile, "src/billing/invoice.rs", (212, 241));
    design.symbol("shop_jobs::export::run_all", SymbolKind::Function);
    design.measure("shop_jobs::export::run_all", MetricId::Lines, 214.0);
    design.symbol(
        "shop::catalog::pricing::resolve_price",
        SymbolKind::Function,
    );
    design.measure(
        "shop::catalog::pricing::resolve_price",
        MetricId::Parameters,
        9.0,
    );
    design.symbol("storefront::orders::OrderService", SymbolKind::Class);
    design.measure("storefront::orders::OrderService", MetricId::Lcom, 5.0);
    for id in [
        "storefront::cart",
        "storefront::checkout",
        "storefront::session",
    ] {
        design.measure(id, MetricId::CycleSize, 3.0);
    }
    let modules = hidden_modules();
    let count = |design: &Design, kinds: &[SymbolKind]| {
        design
            .symbols
            .iter()
            .filter(|symbol| kinds.contains(&symbol.kind))
            .count()
    };
    let functions = 3_204 - count(design, &[SymbolKind::Function, SymbolKind::Method]);
    for index in 0..functions {
        let module = modules
            .get(index % modules.len())
            .cloned()
            .unwrap_or_default();
        let id = format!("{module}::f{index:04}");
        design.node(&id, SymbolKind::Function, 2.0);
        for (metric, value) in [
            (MetricId::Cognitive, 2.0),
            (MetricId::MaxNesting, 1.0),
            (MetricId::Lines, 12.0),
            (MetricId::Parameters, 1.0),
        ] {
            design.measure(&id, metric, value);
        }
    }
    // Every function has a complexity, so that ranks are among all 3,204.
    let measured: std::collections::HashSet<String> = design
        .measurements
        .iter()
        .filter(|measurement| measurement.metric == MetricId::Cyclomatic)
        .map(|measurement| measurement.subject.to_string())
        .collect();
    let unmeasured: Vec<String> = design
        .symbols
        .iter()
        .filter(|symbol| matches!(symbol.kind, SymbolKind::Function | SymbolKind::Method))
        .map(|symbol| symbol.id.to_string())
        .filter(|id| !measured.contains(id))
        .collect();
    for id in unmeasured {
        design.measure(&id, MetricId::Cyclomatic, 1.0);
    }
    let types = 612
        - count(
            design,
            &[SymbolKind::Struct, SymbolKind::Class, SymbolKind::Enum],
        );
    for index in 0..types {
        let module = modules
            .get(index % modules.len())
            .cloned()
            .unwrap_or_default();
        let id = format!("{module}::T{index:03}");
        design.symbol(&id, SymbolKind::Struct);
        design.measure(&id, MetricId::Lcom, 1.0);
    }
}

/// 29 alerts: two errors and 27 warnings. In billing, one error and eight warnings.
fn shop_alerts(design: &mut Design) {
    let mut left = RULES.map(|(_, _, _, count)| count);
    for (id, severity, count) in [
        (
            "shop::billing::invoice::Invoice::reconcile",
            Severity::Warning,
            1,
        ),
        ("shop::billing::tax", Severity::Warning, 4),
        ("shop::billing::ledger", Severity::Warning, 3),
        ("shop::billing::ledger", Severity::Error, 1),
        ("shop::orders::checkout", Severity::Warning, 2),
        ("shop::orders::service", Severity::Warning, 2),
        ("shop::orders::service", Severity::Error, 1),
        ("shop::catalog", Severity::Warning, 5),
        ("storefront::m000", Severity::Warning, 6),
        ("shop_api::m001", Severity::Warning, 4),
    ] {
        for _ in 0..count {
            // Each alert goes to the first rule of its severity that has some left.
            let index = RULES
                .iter()
                .zip(&left)
                .position(|((_, _, of, _), left)| *of == severity && *left > 0)
                .expect("a rule with alerts left");
            let (name, metric, ..) = RULES[index];
            left[index] -= 1;
            design.alerts.push(Alert {
                severity,
                subject: Subject::Symbol(SymbolId::new(id)),
                metric,
                value: 1.0,
                rule: Some(name.to_owned()),
                rule_index: index,
            });
        }
    }
}

/// A file's churn, complexity and hotspot, and its duplication: export.rs is the most
/// copied.
fn measure_file(design: &mut Design, path: &str, (churn, complexity, hotspot): (f64, f64, f64)) {
    let duplication = if path == "shop-jobs/src/export.rs" {
        23.4
    } else {
        1.0
    };
    for (metric, value) in [
        (MetricId::Duplication, duplication),
        (MetricId::Churn, churn),
        (MetricId::Cyclomatic, complexity),
        (MetricId::Hotspot, hotspot),
    ] {
        design.measurements.push(Measurement {
            metric,
            subject: Subject::File(RelativePath::new(path)),
            value,
        });
    }
}

/// Whether a file of the design is Rust.
fn is_rust(path: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|extension| extension == "rs")
}

/// The test code left out: 214 files with 18,902 lines.
fn test_files(design: &mut Design) {
    for index in 0..214_u32 {
        let lines = 88 + u32::from(index < 70);
        let path = format!("tests/t{index:03}.rs");
        design.files.push(AnalyzedFile {
            path: RelativePath::new(&path),
            module: None,
            language: LanguageId::Rust,
            role: CodeRole::Test,
            lines: LineCounts {
                code: lines,
                comment: 0,
                blank: 0,
            },
            test_lines: LineCounts {
                code: lines,
                comment: 0,
                blank: 0,
            },
        });
    }
}

/// A file of the design: its path, its module, and its churn, complexity and hotspot.
type DesignFile = (String, String, f64, f64, f64);

/// The files the Overview ranks by hotspot: path, module, churn, complexity, hotspot.
const RANKED_FILES: [(&str, &str, f64, f64, f64); 8] = [
    (
        "src/billing/invoice.rs",
        "shop::billing::invoice",
        47.0,
        84.0,
        1.0,
    ),
    (
        "src/orders/checkout.rs",
        "shop::orders::checkout",
        41.0,
        68.0,
        0.71,
    ),
    (
        "src/catalog/pricing.rs",
        "shop::catalog::pricing",
        29.0,
        65.0,
        0.48,
    ),
    (
        "storefront/src/cart/reducer.ts",
        "storefront::cart",
        26.0,
        53.0,
        0.35,
    ),
    ("src/billing/tax.rs", "shop::billing::tax", 22.0, 52.0, 0.29),
    (
        "src/orders/service.rs",
        "shop::orders::service",
        24.0,
        39.0,
        0.24,
    ),
    (
        "shop-api/src/routes/orders.rs",
        "shop_api::routes",
        19.0,
        41.0,
        0.20,
    ),
    (
        "storefront/src/checkout/steps.tsx",
        "storefront::checkout",
        17.0,
        36.0,
        0.16,
    ),
];

/// The files the Overview ranks by hotspot, and the rest of the 842: 71% of the lines in
/// Rust and 29% in TypeScript, in four packages.
fn shop_files(design: &mut Design) {
    let ranked = RANKED_FILES;
    let mut files: Vec<DesignFile> = ranked
        .iter()
        .map(|(path, module, churn, complexity, hotspot)| {
            (
                (*path).to_owned(),
                (*module).to_owned(),
                *churn,
                *complexity,
                *hotspot,
            )
        })
        .collect();
    // The rest of the 842 files, in the packages the map frames, out of sight in the
    // modules the other screens leave closed.
    let fillers = [
        (
            "shop",
            "src/f",
            ".rs",
            400,
            &["shop::catalog::model", "shop::notes::digest"][..],
        ),
        (
            "shop_api",
            "shop-api/src/f",
            ".rs",
            120,
            &["shop_api::admin", "shop_api::auth"][..],
        ),
        (
            "shop_jobs",
            "shop-jobs/src/f",
            ".rs",
            63,
            &["shop_jobs::export"][..],
        ),
        (
            "storefront",
            "storefront/src/f",
            ".ts",
            250,
            &["storefront::session", "storefront::orders"][..],
        ),
    ];
    files.push((
        "shop-jobs/src/export.rs".to_owned(),
        "shop_jobs::export".to_owned(),
        1.0,
        1.0,
        0.01,
    ));
    let mut index = 0;
    for (_, prefix, extension, count, modules) in fillers {
        for one in 0..count {
            let module = modules
                .get(one % modules.len())
                .copied()
                .unwrap_or_default();
            files.push((
                format!("{prefix}{index:03}{extension}"),
                module.to_owned(),
                1.0,
                1.0,
                0.01,
            ));
            index += 1;
        }
    }
    test_files(design);
    // The lines of each package, as the map's frames say them, shared out evenly.
    for (package, lines) in [
        ("shop", 52_180_u32),
        ("storefront", 27_960),
        ("shop_api", 10_840),
        ("shop_jobs", 5_430),
    ] {
        let mine: Vec<&DesignFile> = files
            .iter()
            .filter(|file| file.1.split("::").next() == Some(package))
            .collect();
        let count = u32::try_from(mine.len()).expect("a count");
        for (index, (path, module, churn, complexity, hotspot)) in mine.into_iter().enumerate() {
            // The first file takes what is left over.
            let share = lines / count + if index == 0 { lines % count } else { 0 };
            let language = if is_rust(path) {
                LanguageId::Rust
            } else {
                LanguageId::TypeScript
            };
            design.file(path, module, language, share);
            measure_file(design, path, (*churn, *complexity, *hotspot));
        }
    }
}

/// Billing and orders import each other, and billing relates 52 times exactly, 14 times
/// by inference and 4 times ambiguously.
fn shop_relations(design: &mut Design) {
    for (from, to) in [
        ("shop::billing", "shop::orders"),
        ("shop::orders", "shop::billing"),
    ] {
        design.relate(from, to, RelationKind::Imports, Confidence::Exact);
        design.measure(from, MetricId::CycleSize, 2.0);
    }
    // What depends on billing, and what billing depends on, as the diagram draws them.
    for (from, to, confidence) in [
        ("shop_api::routes", "shop::billing", Confidence::Exact),
        ("shop_jobs::export", "shop::billing", Confidence::Exact),
        ("shop_api::admin", "shop::billing", Confidence::Exact),
        ("shop::notes::digest", "shop::billing", Confidence::Exact),
        (
            "shop::inventory::audit",
            "shop::billing",
            Confidence::Inferred,
        ),
        ("shop::billing", "shop::catalog::model", Confidence::Exact),
        ("shop::billing", "shop::inventory", Confidence::Exact),
        (
            "shop::billing",
            "shop::catalog::pricing",
            Confidence::Inferred,
        ),
        ("shop::billing", "shop_api::auth", Confidence::Ambiguous),
    ] {
        design.relate(from, to, RelationKind::Imports, confidence);
    }
    for (from, to) in [
        ("storefront::cart", "storefront::checkout"),
        ("storefront::checkout", "storefront::session"),
        ("storefront::session", "storefront::cart"),
    ] {
        design.relate(from, to, RelationKind::Imports, Confidence::Exact);
    }
    // Elsewhere, enough of each that of all 199 relations, 141 are exact, 48 inferred and
    // 10 ambiguous: the shares Status draws.
    let mut index = 0;
    for (confidence, count) in [
        (Confidence::Exact, 86),
        (Confidence::Inferred, 34),
        (Confidence::Ambiguous, 6),
    ] {
        for _ in 0..count {
            let from = format!("shop_jobs::export::job{index:03}");
            design.symbol(&from, SymbolKind::Function);
            design.relate(
                &from,
                "shop_jobs::export::run_all",
                RelationKind::Calls,
                confidence,
            );
            index += 1;
        }
    }
    let from = "shop::billing::invoice::Invoice::reconcile";
    let mut targets = 0;
    for (confidence, count) in [
        (Confidence::Exact, 44),
        (Confidence::Inferred, 12),
        (Confidence::Ambiguous, 3),
    ] {
        for _ in 0..count {
            let target = format!("shop::billing::rates::helper{targets:02}");
            design.symbol(&target, SymbolKind::Function);
            design.relate(from, &target, RelationKind::Calls, confidence);
            targets += 1;
        }
    }
}

/// The cells of two screens that differ, as `(row, column)`.
fn differences(ours: &str, theirs: &str) -> Vec<(usize, usize)> {
    let ours: Vec<Vec<char>> = ours.lines().map(|line| line.chars().collect()).collect();
    let theirs: Vec<Vec<char>> = theirs.lines().map(|line| line.chars().collect()).collect();
    let mut found = Vec::new();
    for row in 0..ours.len().max(theirs.len()) {
        let a = ours.get(row).cloned().unwrap_or_default();
        let b = theirs.get(row).cloned().unwrap_or_default();
        for column in 0..a.len().max(b.len()) {
            let x = a.get(column).copied().unwrap_or(' ');
            let y = b.get(column).copied().unwrap_or(' ');
            if x != y {
                found.push((row, column));
            }
        }
    }
    found
}

/// The design's text of a screen.
fn design(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../specs/tui/design/screens")
        .join(format!("{name}.txt"));
    std::fs::read_to_string(path).expect("the design screen")
}

/// Opens the interface on an analysis at the design size, and presses keys.
fn opened(analysis: ProjectAnalysis, keys: &str) -> App {
    opened_at(analysis, keys, (120, 36))
}

/// Opens the interface on an analysis at a size, and presses keys.
fn opened_at(analysis: ProjectAnalysis, keys: &str, (columns, rows): (u16, u16)) -> App {
    let mut app = App::new(Startup {
        columns,
        rows,
        theme: ThemeChoice::Dark,
        background: None,
        no_color: false,
        initialized: true,
        editor: false,
    });
    app.take_effects();
    app.analyzed(Ok(analysis));
    app.take_effects();
    press(&mut app, keys);
    app
}

/// Presses keys, setting aside what they ask of the core.
fn press(app: &mut App, keys: &str) {
    for gesture in parse_gestures(keys).expect("keys") {
        if let Gesture::Key(key) = gesture {
            app.update(Input::Key(key));
            app.take_effects();
        }
    }
}

/// The interface 3m 12s into indexing with rust-analyzer, at `extra` milliseconds more.
fn indexing(keys: &str, extra: u64) -> App {
    let mut app = opened(shop().build(), keys);
    press(&mut app, "<Space>p");
    app.precise_checked(Ok(()));
    press(&mut app, "y");
    app.heard(Heard::Indexer("rust-analyzer".to_owned()));
    app.update(Input::Tick(192_000 + extra));
    app
}

/// A file of 300 lines, for Why to show.
fn source_text() -> String {
    let mut text = String::new();
    for line in 1..=300 {
        writeln!(text, "    // line {line}").expect("a line");
    }
    text
}

/// Cells of a design screen the interface may draw differently: rows, columns, and why.
type Allowed<'a> = &'a [(Range<usize>, Range<usize>, &'a str)];

/// Compares a rendering with the design, allowing only the cells listed.
fn assert_matches(app: &mut App, name: &str, allowed: Allowed<'_>) {
    let ours = screen_text(&render(app));
    let theirs = design(name);
    let mut rows: Vec<usize> = differences(&ours, &theirs)
        .into_iter()
        .filter(|(row, column)| {
            !allowed
                .iter()
                .any(|(rows, columns, _)| rows.contains(row) && columns.contains(column))
        })
        .map(|(row, _)| row)
        .collect();
    rows.dedup();
    let ours: Vec<&str> = ours.lines().collect();
    let theirs: Vec<&str> = theirs.lines().collect();
    let mut report = String::new();
    for row in &rows {
        let line = |lines: &[&str]| {
            lines
                .get(*row)
                .map_or(String::new(), |line| (*line).to_owned())
        };
        writeln!(
            report,
            "row {row}\n  ours:   {}\n  design: {}",
            line(&ours),
            line(&theirs)
        )
        .expect("a report");
    }
    assert!(rows.is_empty(), "{name} differs from the design:\n{report}");
}

#[test]
fn explore_matches_the_design() {
    let mut app = opened(
        shop().build(),
        "Mcyclomatic<CR><Space>ezM<C-p>billing<CR>ljljl9jl11kK",
    );
    assert_matches(
        &mut app,
        "explore",
        &[
            (
                4..6,
                41..47,
                "marks roll up to the top of the tree, as specs/tui/explore.md says",
            ),
            (
                16..17,
                41..47,
                "orders holds an error, which rolls up beside its cycle mark",
            ),
            (
                9..18,
                84..115,
                "the rank says how many peers there are, as specs/tui/explore.md says, and \
                 the design's spreads cannot all be drawn from its own values: orders' 27 \
                 falls in a range its cyclomatic spread shows empty",
            ),
        ],
    );
}

/// What the Overview may draw differently from its design, wherever it is analyzed.
const OVERVIEW_ALLOWED: Allowed<'static> = &[
    (
        4..18,
        57..65,
        "the design's spreads cannot all be drawn from its own values: the largest \
                 cycle, of 3, must fall in the top range, not the third",
    ),
    (
        9..10,
        67..111,
        "duplication is measured on files, so the highest is a file, not the \
                 module the design names",
    ),
    (
        15..16,
        67..111,
        "a cycle's modules are named like any other subject, by their last two parts",
    ),
];

#[test]
fn overview_matches_the_design() {
    let mut app = indexing("", 0);
    assert_matches(&mut app, "overview", OVERVIEW_ALLOWED);
}

#[test]
fn overview_at_rest_matches_the_design() {
    let mut app = opened(shop().build(), "");
    assert_matches(&mut app, "overview-idle", OVERVIEW_ALLOWED);
}

#[test]
fn overview_with_a_percentage_matches_the_design() {
    let mut app = indexing("", 0);
    app.heard(Heard::Percent(38));
    assert_matches(&mut app, "overview-percent", OVERVIEW_ALLOWED);
}

#[test]
fn status_matches_the_design() {
    let mut app = opened(shop().build(), "Mcyclomatic<CR><Space>s");
    app.indexers_checked(shop_indexers());
    assert_matches(
        &mut app,
        "status",
        &[
            (
                24..32,
                0..56,
                "macro calls are not counted, so Known limits cannot say how many \
                 functions contain them",
            ),
            (
                27..30,
                56..120,
                "the index does not record its commit yet, so the text does not promise it",
            ),
        ],
    );
}

/// A pane of a screen: its title, and where it is.
#[derive(Debug, PartialEq, Eq)]
struct Pane {
    title: String,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
}

/// The panes of a screen, found by their corners: `╭` or `┏` at the top left.
fn panes(text: &str) -> Vec<Pane> {
    let lines: Vec<Vec<char>> = text.lines().map(|line| line.chars().collect()).collect();
    let mut found = Vec::new();
    for (y, line) in lines.iter().enumerate() {
        for (x, corner) in line.iter().enumerate() {
            if !matches!(corner, '╭' | '┏') {
                continue;
            }
            let Some(right) = (x + 1..line.len()).find(|at| matches!(line[*at], '╮' | '┓'))
            else {
                continue;
            };
            let Some(bottom) = (y + 1..lines.len()).find(|row| {
                lines[*row]
                    .get(x)
                    .is_some_and(|cell| matches!(cell, '╰' | '┗'))
            }) else {
                continue;
            };
            let title: String = line[x + 1..right].iter().collect();
            found.push(Pane {
                title: title
                    .trim_matches(|cell| matches!(cell, '─' | '━' | ' '))
                    .to_owned(),
                x,
                y,
                width: right - x + 1,
                height: bottom - y + 1,
            });
        }
    }
    found
}

/// Checks that a screen has the design's panes in the design's places. Titles are
/// compared too, except those that only say something about the made-up data.
fn assert_panes(app: &mut App, name: &str, data_titles: &[&str]) {
    let ours = panes(&screen_text(&render(app)));
    let theirs = panes(&design(name));
    let place = |pane: &Pane| (pane.x, pane.y, pane.width, pane.height);
    assert_eq!(
        ours.iter().map(place).collect::<Vec<_>>(),
        theirs.iter().map(place).collect::<Vec<_>>(),
        "{name}: the panes are not where the design has them\nours:   {ours:?}\ndesign: {theirs:?}"
    );
    for (mine, design) in ours.iter().zip(&theirs) {
        if !data_titles.contains(&design.title.as_str()) {
            assert_eq!(mine.title, design.title, "{name}: a pane's title");
        }
    }
}

#[test]
fn why_has_the_design_panes() {
    let mut app = opened(shop().build(), "Mcognitive<CR><C-p>reconcile<CR>gd");
    let source = source_text();
    app.loaded("src/billing/invoice.rs", Ok(source));
    assert_panes(&mut app, "why", &[]);
}

#[test]
fn diagram_has_the_design_panes() {
    let mut app = opened(shop().build(), "Mcyclomatic<CR><C-p>billing<CR>gr");
    let ours = panes(&screen_text(&render(&mut app)));
    let theirs = panes(&design("diagram"));
    // The diagram and the inspector, and cards of the design's size in its columns.
    let outer = |list: &[Pane]| {
        list.iter()
            .take(2)
            .map(|pane| (pane.title.clone(), pane.x, pane.y, pane.width, pane.height))
            .collect::<Vec<_>>()
    };
    assert_eq!(outer(&ours), outer(&theirs));
    let card = |pane: &Pane| (pane.width, pane.height);
    for pane in ours.iter().skip(2) {
        assert!(
            theirs
                .iter()
                .skip(2)
                .any(|design| card(design) == card(pane)
                    && design.x == pane.x
                    && design.y == pane.y),
            "a card the design does not place: {pane:?}"
        );
    }
}

#[test]
fn find_has_the_design_panes() {
    let mut app = opened(
        shop().build(),
        "Mcognitive<CR><Space>ffkind = function and cyclomatic > 15 and c",
    );
    // The completions cover the title of the results, and say what the catalog says.
    let covered = "Results ───────────────────────────────│ cognitive      how hard it is to follow                │";
    assert_panes(&mut app, "find", &[covered]);
}

/// Checks the panes a popup opens over, and the popup itself, which is the pane the
/// design places at `popup`.
fn assert_popup(app: &mut App, name: &str, popup: (usize, usize, usize, usize)) {
    let ours = panes(&screen_text(&render(app)));
    let place = |pane: &Pane| (pane.x, pane.y, pane.width, pane.height);
    assert!(
        ours.iter().any(|pane| place(pane) == popup),
        "{name}: no popup at {popup:?}\nours: {ours:?}"
    );
    let theirs = panes(&design(name));
    assert!(
        theirs.iter().any(|pane| place(pane) == popup),
        "{name}: the design has no popup at {popup:?}"
    );
    let first = |list: &[Pane]| list.first().map(place);
    assert_eq!(
        first(&ours),
        first(&theirs),
        "{name}: the pane under the popup"
    );
}

#[test]
fn the_metric_picker_has_the_design_place() {
    let mut app = opened(shop().build(), "Mcyclomatic<CR><C-p>billing<CR><Space>dM");
    assert_popup(&mut app, "metric-picker", (28, 5, 64, 25));
}

#[test]
fn the_relation_picker_has_the_design_place() {
    let mut app = opened(shop().build(), "Mcyclomatic<CR><C-p>billing<CR><Space>dr");
    assert_popup(&mut app, "relation-picker", (30, 8, 60, 13));
}

#[test]
fn the_space_menu_has_the_design_place() {
    let mut app = opened(shop().build(), "Mcyclomatic<CR><Space>e<Space>");
    assert_popup(&mut app, "space-menu", (1, 23, 118, 10));
}

#[test]
fn asking_before_indexing_has_the_design_place() {
    let mut app = opened(shop().build(), "Mcyclomatic<CR><Space>s");
    app.indexers_checked(shop_indexers());
    press(&mut app, "<Space>p");
    app.precise_checked(Ok(()));
    assert_popup(&mut app, "status-confirm", (28, 10, 64, 11));
}

#[test]
fn a_small_terminal_matches_the_design() {
    let mut app = opened_at(
        shop().build(),
        "Mcyclomatic<CR><Space>ezM<C-p>reconcile<CR>",
        (80, 24),
    );
    assert_matches(
        &mut app,
        "small-terminal",
        &[
            (
                3..5,
                56..61,
                "marks roll up to the top of the tree, as specs/tui/explore.md says",
            ),
            (
                14..15,
                56..61,
                "orders holds an error, which rolls up beside its cycle mark",
            ),
            (
                10..16,
                0..80,
                "the design leaves out LineItem, rates, inventory and notes, which its own \
                 explore screen shows",
            ),
        ],
    );
}

#[test]
fn status_while_indexing_has_the_design_panes() {
    let mut app = indexing("Mcyclomatic<CR><Space>s", 0);
    app.indexers_checked(shop_indexers());
    assert_panes(&mut app, "status-running", &[]);
}

#[test]
fn the_first_run_has_the_design_panes() {
    let mut app = App::new(Startup {
        columns: 120,
        rows: 36,
        theme: ThemeChoice::Dark,
        background: None,
        no_color: false,
        initialized: false,
        editor: false,
    });
    app.take_effects();
    app.heard(Heard::Step(Step::Extract));
    app.heard(Heard::Files(514, 842));
    // The project has no name and no partial ranking until files are read.
    assert_panes(
        &mut app,
        "first-run",
        &["shop", "Highest by cyclomatic, so far"],
    );
}

#[test]
fn the_map_has_the_design_frames() {
    let mut app = opened(shop().build(), "<Space>m");
    let ours = panes(&screen_text(&render(&mut app)));
    let theirs = panes(&design("map"));
    // The design places its frames by hand, and gives shop-jobs more room than its lines
    // earn. A frame's area follows its lines, as specs/tui/map.md says, so shop-jobs is
    // too narrow to say its lines.
    let titles = |list: &[Pane]| {
        list.iter()
            .map(|pane| pane.title.clone())
            .collect::<Vec<_>>()
    };
    let mut expected = titles(&theirs);
    if let Some(last) = expected.last_mut() {
        *last = "shop-jobs".to_owned();
    }
    assert_eq!(titles(&ours), expected);
    // Each frame stands to the others as in the design: left of them, or above them.
    let stands = |list: &[Pane]| {
        let mut found = Vec::new();
        for a in list {
            for b in list {
                found.push((a.x + a.width <= b.x, a.y + a.height <= b.y));
            }
        }
        found
    };
    assert_eq!(stands(&ours), stands(&theirs));
}

#[test]
fn moving_on_the_map_goes_to_the_tile_that_way() {
    use ratatui::layout::Rect;
    type Way = fn(Rect, Rect) -> bool;
    let mut app = opened(shop().build(), "<Space>m");
    let scene = app.map_scene(app.map_area());
    let area_of = |app: &App| {
        let selected = app.selected().expect("a selected tile");
        scene
            .tiles
            .iter()
            .find(|tile| tile.id == selected)
            .map(|tile| tile.area)
            .expect("a tile drawn for the selection")
    };
    // The first move selects a tile.
    press(&mut app, "l");
    let ways: [(&str, Way); 4] = [
        ("l", |from, to| to.x >= from.x + from.width),
        ("j", |from, to| to.y >= from.y + from.height),
        ("h", |from, to| to.x + to.width <= from.x),
        ("k", |from, to| to.y + to.height <= from.y),
    ];
    let mut moves = 0;
    for (key, that_way) in ways {
        let from = area_of(&app);
        press(&mut app, key);
        let to = area_of(&app);
        if to != from {
            assert!(that_way(from, to), "{key} went from {from:?} to {to:?}");
            moves += 1;
        }
    }
    assert!(moves >= 2, "only {moves} of the moves went anywhere");
}

#[test]
fn the_rest_of_a_column_opens_beside_its_card() {
    let mut app = opened(shop().build(), "Mcyclomatic<CR><C-p>billing<CR><Space>d");
    let card = app
        .scene()
        .and_then(|(_, scene)| scene.more.first().map(|more| more.area))
        .expect("a +N more card");
    // A click finds its target among what was drawn last.
    render(&mut app);
    app.update(Input::Mouse {
        kind: crate::input::MouseKind::Click,
        column: card.x + 1,
        row: card.y + 1,
    });
    let ours = panes(&screen_text(&render(&mut app)));
    let theirs = panes(&design("more-nodes"));
    let size = |pane: &Pane| (pane.width, pane.height);
    // The design's popup, 50 by 9 for three nodes, beside its card and three rows up.
    let popup = theirs
        .iter()
        .find(|pane| size(pane) == (50, 9))
        .expect("the design's popup");
    let mine = ours
        .iter()
        .find(|pane| size(pane) == size(popup))
        .expect("our popup");
    let x = usize::from(card.x + card.width + 1);
    let y = usize::from(card.y.saturating_sub(3));
    assert_eq!((mine.x, mine.y), (x, y));
}

#[test]
fn the_right_click_menu_opens_where_it_was_clicked() {
    let mut app = opened(
        shop().build(),
        "Mcyclomatic<CR><Space>ezM<C-p>reconcile<CR>",
    );
    render(&mut app);
    // The design right-clicks reconcile, at row 8 and column 24.
    app.update(Input::Mouse {
        kind: crate::input::MouseKind::RightClick,
        column: 24,
        row: 8,
    });
    assert_popup(&mut app, "context-menu", (24, 8, 38, 11));
}

#[test]
fn alerts_has_the_design_panes() {
    let mut app = opened(shop().build(), "Mcyclomatic<CR><Space>a");
    assert_panes(&mut app, "alerts", &["23 hits, worst first"]);
}

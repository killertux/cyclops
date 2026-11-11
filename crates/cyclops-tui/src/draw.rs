//! What the interface looks like: the shell, each view, the inspector and the popups.
//! See `specs/tui/design/`.

use crate::app::{App, MIN_COLUMNS, MIN_ROWS, Running, SIDE_BY_SIDE, hints, picked, space_menu};
use crate::app::{CONTEXT_MENU, MapScene, MapTile, TreeRow, relation_choices, shows_source};
use crate::canvas::inner;
use crate::canvas::{Canvas, fit, fit_start, width};
use crate::editor::{FIELDS, RuleEditor};
use crate::scene::{BoxRow, Card, Route, routes};
use crate::state::{
    OVERVIEW_METRICS, Pane, Popup, PromptKind, Scroll, Section, Target, View, metric_label,
};
use crate::tokens::Token;
use crate::treemap::steps;
use cyclops_application::analysis::ProjectAnalysis;
use cyclops_application::diagram::Level;
use cyclops_application::ports::{Partial, Step};
use cyclops_application::queries::RuleSpread;
use cyclops_domain::alert::AlertRule;
use cyclops_domain::metric::{Heat, MetricId, Scope, Subject};
use cyclops_domain::relation::Confidence;
use cyclops_domain::relation::RelationKind;
use cyclops_domain::settings::RawValue;
use cyclops_domain::settings::Severity;
use cyclops_domain::symbol::CodeRole;
use cyclops_domain::symbol::SymbolId;
use cyclops_domain::symbol::{Symbol, SymbolKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use std::collections::HashMap;

/// Draws the interface into a buffer, and remembers where the mouse can point.
pub fn draw(app: &mut App, buffer: &mut Buffer) {
    let hits = {
        let mut canvas = Canvas::new(buffer, app.theme());
        canvas.ground();
        if app.too_small() {
            too_small(&mut canvas);
        } else {
            shell(app, &mut canvas);
        }
        canvas.hits
    };
    app.set_hits(hits);
}

fn too_small(canvas: &mut Canvas<'_>) {
    let area = canvas.area();
    let message = format!("The terminal is too small. Cyclops needs {MIN_COLUMNS} x {MIN_ROWS}.");
    let y = area.height / 2;
    let x = area.width.saturating_sub(width(&message)) / 2;
    canvas.text(x, y, &message, Token::Fg, area);
}

fn shell(app: &App, canvas: &mut Canvas<'_>) {
    let area = canvas.area();
    tab_bar(app, canvas, area);
    breadcrumb(app, canvas, area);
    let body = Rect {
        x: area.x,
        y: area.y + 2,
        width: area.width,
        height: area.height.saturating_sub(4),
    };
    if let Some(editor) = app.editor() {
        rule_editor(app, editor, canvas, body);
    } else {
        match app.view() {
            View::Overview => overview(app, canvas, body),
            View::Explore if app.why_source().is_some() => why_view(app, canvas, body),
            View::Explore => with_inspector(app, canvas, body, 58, structure),
            View::Diagram => with_inspector(app, canvas, body, 36, diagram),
            View::Map => with_inspector(app, canvas, body, 36, map),
            View::Find => with_inspector(app, canvas, body, 40, find),
            View::Alerts => alerts(app, canvas, body),
            View::Status => status(app, canvas, body),
        }
    }
    status_line(app, canvas, area);
    hint_line(app, canvas, area);
    popups(app, canvas, body);
}

fn tab_bar(app: &App, canvas: &mut Canvas<'_>, area: Rect) {
    canvas.text(area.x + 2, area.y, "◉", Token::Accb, area);
    canvas.text(area.x + 4, area.y, "cyclops", Token::B, area);
    let mut x = area.x + 13;
    let last = View::ALL.len() - 1;
    for (index, view) in View::ALL.into_iter().enumerate() {
        let name = view.name();
        let start = x;
        let gap = if index == last { " " } else { "  " };
        if view == app.view() {
            canvas.text(x - 1, area.y, &format!(" {name} "), Token::On, area);
        } else {
            let mut letters = name.chars();
            let first: String = letters.next().map(String::from).unwrap_or_default();
            let after = canvas.text(x, area.y, &first, Token::Key, area);
            canvas.text(
                after,
                area.y,
                &format!("{}{gap}", letters.as_str()),
                Token::Dim,
                area,
            );
        }
        x += width(name) + 2;
        canvas.hit(Rect::new(start, area.y, width(name), 1), Target::Tab(view));
    }
    let summary = match app.analysis() {
        Some(analysis) => {
            let (files, lines) = analysis.counted_size();
            let project = app
                .focus()
                .and_then(|focus| app.symbol(focus))
                .map(|symbol| root_name(app, symbol))
                .unwrap_or_default();
            format!(
                "{project} · {} files · {} lines",
                grouped(files),
                grouped(usize::try_from(lines).unwrap_or_default())
            )
        }
        None if app.analyzing() => "first run".to_owned(),
        None => String::new(),
    };
    let right = area.x + area.width - 1;
    if !summary.is_empty() && right > x + width(&summary) {
        canvas.text_right(right, area.y, &summary, Token::Faint, area);
    }
}

fn root_name(app: &App, symbol: &Symbol) -> String {
    let mut current = symbol;
    while let Some(parent) = current
        .parent
        .as_ref()
        .and_then(|parent| app.symbol(parent))
    {
        current = parent;
    }
    current.name.clone()
}

/// A count with commas between thousands.
fn grouped(count: usize) -> String {
    let digits = count.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

fn breadcrumb(app: &App, canvas: &mut Canvas<'_>, area: Rect) {
    let y = area.y + 1;
    let ids = app.breadcrumb_ids();
    let names = app.breadcrumb();
    let mut parts: Vec<(String, Option<SymbolId>)> =
        names.into_iter().zip(ids.into_iter().map(Some)).collect();
    let mut right = String::new();
    if let Some(editor) = app.editor() {
        parts = vec![
            ("New rule from ".to_owned(), None),
            (short_id(&editor.from), None),
        ];
        if editor.existing.is_some() {
            parts = vec![("Rule › ".to_owned(), None), (editor.from.clone(), None)];
        }
    } else {
        match app.view() {
            View::Find | View::Status => {
                parts.truncate(1);
                parts.push((app.view().name().to_lowercase(), None));
                if app.view() == View::Find {
                    "filter stays on across views".clone_into(&mut right);
                }
            }
            View::Alerts => {
                let rule = app
                    .listed_rules()
                    .get(app.selected_rule())
                    .map(|(_, rule)| rule.title());
                parts = vec![("Rule".to_owned(), None)];
                if let Some(rule) = rule {
                    parts.push((rule, None));
                }
                right = if app.by_location() {
                    "by location · v by rule".to_owned()
                } else {
                    "by rule · v by location".to_owned()
                };
            }
            View::Explore if app.why_source().is_some() => parts.push(("why".to_owned(), None)),
            _ => {}
        }
    }
    let starts_editor = app.editor().is_some();
    // Narrow, only the last two parts show, after an ellipsis.
    let narrow = area.width < SIDE_BY_SIDE;
    if narrow && !starts_editor && parts.len() > 2 {
        let kept = parts.split_off(parts.len() - 2);
        parts = std::iter::once(("…".to_owned(), None))
            .chain(kept)
            .collect();
    }
    let mut x = area.x + 1;
    let last = parts.len().saturating_sub(1);
    for (index, (part, id)) in parts.iter().enumerate() {
        if index > 0 && !starts_editor {
            x = canvas.text(x, y, " › ", Token::Dim, area);
        }
        let token = if index == last { Token::B } else { Token::Dim };
        let start = x;
        x = canvas.text(x, y, part, token, area);
        if let Some(id) = id {
            canvas.hit(
                Rect::new(start, y, x - start, 1),
                Target::Breadcrumb(id.clone()),
            );
        }
    }
    if right.is_empty() && !matches!(app.view(), View::Status) {
        right = breadcrumb_kind(app, narrow);
    }
    if let Some(editor) = app.editor().filter(|editor| editor.existing.is_none()) {
        right = format!(
            "prefilled from {}",
            editor.values.get(1).cloned().unwrap_or_default()
        );
    }
    canvas.text_right(area.x + area.width - 1, y, &right, Token::Faint, area);
}

/// The right of row 1: the focus's kind, then what is selected or where in the trail.
fn breadcrumb_kind(app: &App, narrow: bool) -> String {
    let Some(focus) = app.focus() else {
        return String::new();
    };
    let mut right = kind_name(app, focus);
    if app.view() == View::Map
        && let Some(selected) = app
            .selected()
            .filter(|selected| Some(selected) != app.focus())
    {
        let name = app
            .symbol(&selected)
            .map(|symbol| symbol.name.clone())
            .unwrap_or_default();
        right = format!("{right} · selected {name}");
    } else if let Some(selected) = app
        .selected()
        .filter(|selected| Some(selected) != app.focus() && app.view() == View::Diagram)
    {
        let name = app
            .symbol(&selected)
            .map(|symbol| symbol.name.clone())
            .unwrap_or_default();
        right = format!("{right} · selected {name}");
    } else {
        let (position, length) = app.trail();
        if length > 1 && !narrow {
            right = format!("{right} · trail {position}/{length}");
        }
    }
    right
}

fn status_line(app: &App, canvas: &mut Canvas<'_>, area: Rect) {
    let y = area.y + area.height.saturating_sub(2);
    let line = Rect {
        x: area.x,
        y,
        width: area.width,
        height: 1,
    };
    canvas.fill(line, Token::Chip);
    let narrow = area.width < SIDE_BY_SIDE;
    let mode = format!(" {} ", app.mode().name());
    let mode_token = if app.mode() == crate::state::Mode::Normal {
        Token::Mode
    } else {
        Token::On
    };
    let mut x = canvas.text(area.x, y, &mode, mode_token, line);
    let lens = app.lens();
    let tests = if lens.tests { "on" } else { "off" };
    let layer = if app.precise_layer() {
        "precise"
    } else {
        "syntax"
    };
    let chip = |canvas: &mut Canvas<'_>,
                x: u16,
                name: &'static str,
                label: &str,
                value: &str,
                token: Token| {
        let start = x;
        let after = canvas.text(x, y, label, Token::Chip, line);
        let end = canvas.text(after, y, value, token, line);
        canvas.hit(Rect::new(start, y, end - start, 1), Target::Chip(name));
        end
    };
    if narrow {
        x = chip(canvas, x, "metric", " ", app.metric_label(), Token::Chipa);
        let filter = lens
            .filter
            .as_ref()
            .map_or_else(|| "no filter".to_owned(), |filter| filter.text.clone());
        x = chip(canvas, x, "filter", "  ", &filter, Token::Chip);
        chip(canvas, x, "tests", "  tests ", tests, Token::Chip);
    } else {
        x = chip(
            canvas,
            x,
            "metric",
            " metric ",
            app.metric_label(),
            Token::Chipa,
        );
        let (filter, filter_token) = lens
            .filter
            .as_ref()
            .map_or(("none", Token::Chipd), |filter| {
                (filter.text.as_str(), Token::Chipa)
            });
        x = chip(canvas, x, "filter", "  filter ", filter, filter_token);
        x = chip(canvas, x, "tests", "  tests ", tests, Token::Chip);
        canvas.text(x, y, &format!("  layer {layer}"), Token::Chip, line);
    }
    let mut parts: Vec<(String, Token, Option<&'static str>)> = Vec::new();
    if let Some(analysis) = app.analysis() {
        let alerts = analysis.alerts().len();
        let alerts = if narrow {
            format!("{alerts} !")
        } else {
            plural(alerts, "alert")
        };
        parts.push((alerts, Token::Chiph, Some("alerts")));
        let problems = plural(analysis.problems().len(), "problem");
        parts.push((problems, Token::Chip, Some("problems")));
    } else if app.failure().is_some() {
        parts.push((plural(1, "problem"), Token::Chip, Some("problems")));
    }
    if !narrow {
        let freshness = freshness(app);
        parts.push((freshness.0, freshness.1, None));
    }
    let total: u16 = parts.iter().map(|(text, ..)| width(text)).sum::<u16>()
        + 2 * u16::try_from(parts.len().saturating_sub(1)).unwrap_or(0);
    let end = (area.x + area.width).saturating_sub(1);
    let mut x = end.saturating_sub(total);
    for (text, token, chip) in parts {
        let start = x;
        x = canvas.text(x, y, &text, token, line);
        if let Some(chip) = chip {
            canvas.hit(Rect::new(start, y, x - start, 1), Target::Chip(chip));
        }
        x += 2;
    }
}

/// How fresh the analysis is, as the status line says it.
fn freshness(app: &App) -> (String, Token) {
    match app.running_analysis() {
        Some(running) if running.precise => {
            let progress = running.percent.map_or_else(
                || elapsed(app.now().saturating_sub(running.since)),
                |percent| format!("{percent}%"),
            );
            (format!("indexing {progress}"), Token::Chipa)
        }
        Some(running) => {
            let (done, of) = running.files;
            let text = if of > 0 {
                format!("{done} of {of} files")
            } else {
                "analyzing".to_owned()
            };
            (text, Token::Chipa)
        }
        None if app.stale() => ("stale".to_owned(), Token::Chipa),
        None => ("fresh".to_owned(), Token::Chipd),
    }
}

fn plural(count: usize, word: &str) -> String {
    if count == 1 {
        format!("1 {word}")
    } else {
        format!("{} {word}s", grouped(count))
    }
}

/// The keys of Why, which takes the whole of Explore.
const WHY_HINTS: &[(&str, &str)] = &[
    ("j k", "scroll"),
    ("n N", "next decision"),
    ("Tab", "next pane"),
    ("gr", "impact"),
    ("gf", "open line in editor"),
    ("Ctrl-o", "back"),
];

/// The keys of a diagram of types, where `Enter` opens a class box.
const TYPE_HINTS: &[(&str, &str)] = &[
    ("hjkl", "move"),
    ("Enter", "expand"),
    ("c", "center"),
    ("< >", "depth"),
    ("r", "relations"),
    ("M", "overlay"),
    ("gd", "why"),
];

/// The keys of Explore when it is too narrow for the inspector beside it.
const NARROW_EXPLORE_HINTS: &[(&str, &str)] = &[
    ("hjkl", "move"),
    ("Enter", "open"),
    ("K", "full inspector"),
    ("gd", "why"),
];

/// The most useful keys now: Why's while it shows, a diagram of types' while it shows,
/// and the view's otherwise.
fn hints_now(app: &App) -> &'static [(&'static str, &'static str)] {
    match app.view() {
        View::Explore if app.why_source().is_some() => WHY_HINTS,
        View::Explore if app.size().0 < SIDE_BY_SIDE => NARROW_EXPLORE_HINTS,
        View::Diagram if app.diagram_level() == Level::Type => TYPE_HINTS,
        view => view_hints(view),
    }
}

/// The most useful keys of each view.
fn view_hints(view: View) -> &'static [(&'static str, &'static str)] {
    match view {
        View::Overview => &[
            ("j k", "pick metric"),
            ("Tab", "next pane"),
            ("Enter", "open"),
            ("M", "metric"),
            ("f", "filter"),
            ("Ctrl-p", "jump"),
        ],
        View::Explore => &[
            ("hjkl", "move"),
            ("Enter", "open"),
            ("K", "inspector"),
            ("H L", "sections"),
            ("gd", "why"),
            ("gr", "impact"),
            ("M", "metric"),
            ("f", "filter"),
        ],
        View::Diagram => &[
            ("hjkl", "move"),
            ("Enter", "drill in"),
            ("c", "center"),
            ("< >", "depth"),
            ("r", "relations"),
            ("M", "overlay"),
            (":export", "save view"),
        ],
        View::Map => &[
            ("hjkl", "move between tiles"),
            ("Enter", "zoom in"),
            ("-", "zoom out"),
            ("M", "color by"),
            ("K", "inspect"),
            ("gd", "why"),
        ],
        View::Find => &[
            ("f", "filter"),
            ("s", "sort"),
            ("Enter", "open"),
            ("yy", "copy"),
            ("M", "metric"),
        ],
        View::Alerts => &[
            ("j k", "move"),
            ("Enter", "go to symbol"),
            ("]a", "next hit"),
            ("A", "new rule"),
            ("e", "edit"),
            ("x", "disable"),
            ("v", "group"),
        ],
        View::Status => &[
            ("Enter", "open file"),
            ("]p", "next problem"),
            ("Space p", "precise"),
            ("Space t", "test code"),
            ("Space r", "refresh"),
        ],
    }
}

fn hint_line(app: &App, canvas: &mut Canvas<'_>, area: Rect) {
    let y = area.y + area.height.saturating_sub(1);
    let line = Rect {
        x: area.x,
        y,
        width: area.width,
        height: 1,
    };
    if app.view() == View::Find
        && app
            .prompt()
            .is_some_and(|prompt| prompt.kind == PromptKind::Filter)
    {
        let mut x = area.x + 1;
        for (key, meaning) in [
            ("Enter", "apply"),
            ("Tab", "complete"),
            ("Up Down", "history"),
            ("Esc", "back to results"),
        ] {
            x = canvas.text(x, y, key, Token::Acc, line);
            x = canvas.text(x + 1, y, meaning, Token::Dim, line) + 2;
        }
        return;
    }
    if let Some(prompt) = app.prompt() {
        let label = match prompt.kind {
            PromptKind::Command => ":",
            PromptKind::Filter => "filter: ",
            PromptKind::Search => "/",
            PromptKind::Jump => "jump to: ",
        };
        let mut x = canvas.text(area.x, y, label, Token::Acc, line);
        x = canvas.text(x, y, &prompt.text, Token::Fg, line);
        x = canvas.text(x, y, "█", Token::Acc, line);
        if let Some(error) = &prompt.error {
            x = canvas.text(x + 2, y, error, Token::H2, line);
        }
        let keys = match prompt.kind {
            PromptKind::Command => "Tab complete   Enter run   Esc cancel",
            PromptKind::Filter => "Enter apply   Esc cancel",
            PromptKind::Search => "Enter search   Esc cancel",
            PromptKind::Jump => "Enter jump   Esc cancel",
        };
        canvas.text(x + 3, y, keys, Token::Dim, line);
        return;
    }
    if matches!(app.popup(), Some(Popup::SpaceMenu)) {
        canvas.text(area.x + 1, y, "Space", Token::Acc, line);
        canvas.text(
            area.x + 7,
            y,
            "press a key, or click one. Esc closes.",
            Token::Dim,
            line,
        );
        return;
    }
    if let Some(message) = app.message().or_else(|| app.hovering()) {
        canvas.text(area.x + 1, y, message, Token::Fg, line);
    } else {
        let mut x = area.x + 1;
        for (key, meaning) in hints_now(app) {
            x = canvas.text(x, y, key, Token::Acc, line);
            x = canvas.text(x + 1, y, meaning, Token::Dim, line) + 2;
        }
    }
    let right = "Space menu   ? keys";
    let start = (area.x + area.width).saturating_sub(width(right) + 1);
    canvas.fill(
        Rect {
            x: start.saturating_sub(1),
            y,
            width: width(right) + 2,
            height: 1,
        },
        Token::Fg,
    );
    let x = canvas.text(start, y, "Space", Token::Acc, line);
    let x = canvas.text(x + 1, y, "menu", Token::Dim, line);
    let x = canvas.text(x + 3, y, "?", Token::Acc, line);
    canvas.text(x + 1, y, "keys", Token::Dim, line);
}

/// Splits the body between a view's main pane and the inspector, when it is open.
fn with_inspector(
    app: &App,
    canvas: &mut Canvas<'_>,
    body: Rect,
    inspector_width: u16,
    main: fn(&App, &mut Canvas<'_>, Rect, bool),
) {
    let panes = app.panes();
    let active = app.pane();
    if app.inspector().is_none() {
        main(app, canvas, body, active == panes.first().copied());
        return;
    }
    if body.width < SIDE_BY_SIDE {
        inspector(app, canvas, body, true);
        return;
    }
    let side = app
        .inspector_width_for(inspector_width)
        .min(body.width.saturating_sub(20));
    let left = Rect {
        width: body.width - side,
        ..body
    };
    let right = Rect {
        x: body.x + body.width - side,
        width: side,
        ..body
    };
    main(app, canvas, left, active == panes.first().copied());
    inspector(app, canvas, right, active == Some(Pane::Inspector));
    canvas.hit(
        Rect::new(right.x, right.y, 1, right.height),
        Target::Divider,
    );
}

fn heat_token(heat: Option<Heat>) -> Token {
    match heat {
        None => Token::Fg,
        Some(Heat::Quiet) => Token::H0,
        Some(Heat::Mild) => Token::H1,
        Some(Heat::High) => Token::H2,
        Some(Heat::Severe) => Token::H3,
    }
}

fn scope_of(symbol: &Symbol) -> Option<Scope> {
    match symbol.kind {
        SymbolKind::Function | SymbolKind::Method => Some(Scope::Function),
        SymbolKind::Module => Some(Scope::Module),
        SymbolKind::Project | SymbolKind::Package => Some(Scope::Project),
        kind if kind.is_type() => Some(Scope::Type),
        _ => None,
    }
}

/// The token of a symbol's value, by its heat.
fn value_token(analysis: &ProjectAnalysis, symbol: &Symbol, metric: MetricId) -> Token {
    let value = analysis.value(&Subject::Symbol(symbol.id.clone()), metric);
    let heat = value
        .zip(scope_of(symbol))
        .and_then(|(value, scope)| metric.descriptor().heat_of(scope, value));
    if value.is_none() {
        Token::Dim
    } else {
        heat_token(heat)
    }
}

/// A bar of `cells` cells, filled in proportion and rounded down to the half cell, so
/// that a bar is never longer than its value.
fn bar(value: f64, highest: f64, cells: u16) -> (String, String) {
    let share = if highest > 0.0 { value / highest } else { 0.0 };
    let target = share.clamp(0.0, 1.0) * f64::from(cells) * 2.0;
    let halves = (0..=cells * 2)
        .rev()
        .find(|step| f64::from(*step) <= target + 1e-9)
        .unwrap_or(0);
    let full = halves / 2;
    let mut filled = "━".repeat(usize::from(full));
    if halves % 2 == 1 {
        filled.push('╸');
    }
    let used = full + halves % 2;
    let rest = "─".repeat(usize::from(cells.saturating_sub(used)));
    (filled, rest)
}

fn structure(app: &App, canvas: &mut Canvas<'_>, area: Rect, active: bool) {
    let inside = canvas.pane(area, "Structure", active);
    let Some(analysis) = app.analysis() else {
        nothing_yet(app, canvas, inside);
        return;
    };
    let metric = app.lens().metric;
    let right = inside.x + inside.width;
    // Narrow, there is no heading, the bars are longer, and the selection is summed up
    // at the bottom in place of an inspector.
    let narrow = app.size().0 < SIDE_BY_SIDE;
    let (bar_cells, bar_start, value_end, mark_column) = if narrow {
        (
            12,
            right.saturating_sub(14),
            right.saturating_sub(15),
            right.saturating_sub(21),
        )
    } else {
        (
            10,
            right.saturating_sub(11),
            right.saturating_sub(12),
            right.saturating_sub(18),
        )
    };
    let top = if narrow { inside.y } else { inside.y + 1 };
    if !narrow {
        let heading = format!("{} (highest inside)", metric_label(metric));
        canvas.text(inside.x + 1, inside.y, "name", Token::Faint, inside);
        canvas.text_right(right, inside.y, &heading, Token::Faint, inside);
    }
    let rows = app.tree();
    let marks = app.tree_marks();
    let highest = rows
        .iter()
        .filter_map(|row| analysis.value(&Subject::Symbol(row.symbol.id.clone()), metric))
        .fold(0.0, f64::max);
    let room = if narrow {
        usize::from(inside.height.saturating_sub(5))
    } else {
        usize::from(inside.height.saturating_sub(1))
    };
    let selected = app
        .focus()
        .and_then(|focus| rows.iter().position(|row| &row.symbol.id == focus));
    let base = selected.map_or(0, |selected| {
        selected.saturating_sub(room.saturating_sub(1))
    });
    let (first, further) = scroll_window(app, Scroll::Tree, base, rows.len(), room);
    let list = Rect {
        y: top,
        height: u16::try_from(room).unwrap_or(u16::MAX),
        ..inside
    };
    canvas.hit(list, Target::List(Scroll::Tree, further));
    for (index, row) in rows.iter().enumerate().skip(first).take(room) {
        let y = top + u16::try_from(index - first).unwrap_or_default();
        let line = Rect {
            y,
            height: 1,
            ..inside
        };
        if Some(index) == selected {
            canvas.fill(line, Token::Sel);
        }
        let name_room = Rect {
            width: mark_column.saturating_sub(line.x + 1),
            ..line
        };
        tree_name(analysis, canvas, row, name_room);
        if let Some(mark) = marks.get(&row.symbol.id) {
            let token = if mark.starts_with("!!") {
                Token::H3
            } else {
                Token::H2
            };
            canvas.text(mark_column, y, mark, token, line);
        }
        let value = app.shown_value(&row.symbol.id, metric);
        let token = value_token(analysis, row.symbol, metric);
        canvas.text_right(value_end, y, &value, token, line);
        if let Some(number) = analysis.value(&Subject::Symbol(row.symbol.id.clone()), metric) {
            let (filled, rest) = bar(number, highest, bar_cells);
            let x = canvas.text(bar_start, y, &filled, token, line);
            canvas.text(x, y, &rest, Token::Track, line);
        }
        canvas.hit(line, Target::Symbol(row.symbol.id.clone()));
    }
    if narrow {
        let bottom = Rect {
            y: inside.y + inside.height.saturating_sub(4),
            height: 3.min(inside.height),
            ..inside
        };
        selection_strip(app, analysis, canvas, bottom);
    }
}

/// The first row a list shows: where following the selection puts it, moved on by the
/// wheel as far as the list goes. Also says how much further the wheel could move it.
fn scroll_window(
    app: &App,
    scroll: Scroll,
    base: usize,
    len: usize,
    room: usize,
) -> (usize, usize) {
    let most = len.saturating_sub(room).max(base);
    let first = (base + app.scrolled(scroll)).min(most);
    (first, most - base)
}

/// A row's name in the tree: indented by its depth, with an arrow when it opens, and
/// with its kind when a sibling shares its name.
fn tree_name(analysis: &ProjectAnalysis, canvas: &mut Canvas<'_>, row: &TreeRow<'_>, room: Rect) {
    let indent = u16::try_from(row.depth * 2).unwrap_or(u16::MAX);
    let arrow = match row.open {
        Some(true) => "▾ ",
        Some(false) => "▸ ",
        None => "  ",
    };
    let token = if row.depth == 0 { Token::B } else { Token::Fg };
    let name = format!("{arrow}{}", row.symbol.name);
    let end = canvas.text(room.x + 1 + indent, room.y, &name, token, room);
    if shares_its_name(analysis, row.symbol) {
        canvas.text(end + 1, room.y, row.symbol.kind.name(), Token::Dim, room);
    }
}

/// Whether another symbol of the same parent has this symbol's name, as a getter and its
/// field do.
fn shares_its_name(analysis: &ProjectAnalysis, symbol: &Symbol) -> bool {
    symbol.parent.as_ref().is_some_and(|parent| {
        analysis
            .view()
            .children(parent)
            .any(|other| other.id != symbol.id && other.name == symbol.name)
    })
}

/// The selection in a line or two, where there is no room for an inspector: its name, the
/// lens's metric with its rank, its size, and its alerts.
fn selection_strip(app: &App, analysis: &ProjectAnalysis, canvas: &mut Canvas<'_>, area: Rect) {
    let Some(symbol) = app.selected().and_then(|id| app.symbol(&id)) else {
        return;
    };
    let x = area.x + 1;
    let divider = "─".repeat(usize::from(area.width.saturating_sub(2)));
    canvas.text(x, area.y, &divider, Token::Faint, area);
    let subject = Subject::Symbol(symbol.id.clone());
    let metric = app.lens().metric;
    let mut parts = vec![symbol.name.clone()];
    let mut shown = vec![metric];
    shown.extend(
        [MetricId::Cognitive, MetricId::Lines]
            .into_iter()
            .filter(|other| *other != metric),
    );
    for (index, each) in shown.into_iter().enumerate() {
        let Some(value) = analysis.value(&subject, each) else {
            continue;
        };
        parts.push(format!(
            "{} {}",
            metric_label(each),
            crate::state::shown(each, value)
        ));
        if index == 0
            && let Some(rank) = analysis.rank(&subject, each)
        {
            parts.push(format!("#{} of {}", rank.position, grouped(rank.of)));
        }
    }
    canvas.text(x, area.y + 1, &parts.join("  "), Token::Fg, area);
    let alerts: Vec<String> = analysis
        .alerts()
        .iter()
        .filter(|alert| alert.subject == subject)
        .filter_map(|alert| {
            analysis
                .rules()
                .get(alert.rule_index)
                .map(|rule| (alert, rule.title()))
        })
        .map(|(alert, title)| {
            let mark = if alert.severity == Severity::Error {
                "!!"
            } else {
                "!"
            };
            format!("{mark} {title}")
        })
        .collect();
    if let Some(first) = alerts.first() {
        canvas.text(x, area.y + 2, first, Token::H2, area);
    }
}

fn nothing_yet(app: &App, canvas: &mut Canvas<'_>, area: Rect) {
    let text = app
        .failure()
        .map(str::to_owned)
        .or_else(|| {
            app.analysis()
                .and_then(|analysis| analysis.problems().first())
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| "Analyzing…".to_owned());
    canvas.text(area.x + 1, area.y, &text, Token::Fg, area);
}

fn inspector(app: &App, canvas: &mut Canvas<'_>, area: Rect, active: bool) {
    let selected = app.selected();
    let name = selected
        .as_ref()
        .and_then(|id| app.symbol(id))
        .map(|symbol| symbol.name.clone())
        .unwrap_or_default();
    let title = if selected.as_ref() == app.focus() || selected.is_none() {
        "Inspector".to_owned()
    } else {
        format!("Selected: {name}")
    };
    let inside = canvas.pane(area, &title, active);
    let (Some(analysis), Some(id)) = (app.analysis(), selected) else {
        return;
    };
    let Some(symbol) = analysis.view().symbol(&id) else {
        return;
    };
    let right = inside.x + inside.width - 1;
    let open = app.inspector().unwrap_or(Section::Metrics);
    canvas.text(inside.x + 1, inside.y, &symbol.name, Token::B, inside);
    // Narrow, the kind goes with the place, and only the open section is named.
    let narrow = inside.width < 50;
    if narrow {
        let place = symbol.location.as_ref().map_or_else(
            || {
                analysis
                    .value(&Subject::Symbol(symbol.id.clone()), MetricId::Lines)
                    .map_or_else(String::new, |lines| {
                        format!(" · {} lines", crate::state::shown(MetricId::Lines, lines))
                    })
            },
            |location| format!(" · {}:{}", location.file, location.span.start_line),
        );
        let line = format!("{}{place}", kind_name(app, &id));
        canvas.text(inside.x + 1, inside.y + 1, &line, Token::Dim, inside);
        let start = inside.x + 1;
        let end = canvas.text(
            start,
            inside.y + 3,
            &format!(" {} ", open.name()),
            Token::On,
            inside,
        );
        canvas.hit(
            Rect::new(start, inside.y + 3, end - start, 1),
            Target::Section(open.step(true)),
        );
        let end = canvas.text(end + 1, inside.y + 3, "H L", Token::Acc, inside);
        canvas.text(end, inside.y + 3, " sections", Token::Dim, inside);
    } else {
        canvas.text_right(right, inside.y, &kind_name(app, &id), Token::Dim, inside);
        let place = symbol.location.as_ref().map_or_else(
            || folder_of(analysis, &symbol.id),
            |location| location.file.to_string(),
        );
        canvas.text(inside.x + 1, inside.y + 1, &place, Token::Dim, inside);
    }
    let mut x = inside.x + 1;
    for section in Section::ALL.into_iter().filter(|_| !narrow) {
        let start = x;
        let end = if section == open {
            canvas.text(
                x,
                inside.y + 3,
                &format!(" {} ", section.name()),
                Token::On,
                inside,
            )
        } else {
            canvas.text(x, inside.y + 3, section.name(), Token::Dim, inside)
        };
        canvas.hit(
            Rect::new(start, inside.y + 3, end.saturating_sub(start), 1),
            Target::Section(section),
        );
        x = end + if section == open { 2 } else { 3 };
    }
    let body = Rect {
        y: inside.y + 5,
        height: inside.height.saturating_sub(5),
        ..inside
    };
    match open {
        Section::Metrics => inspector_metrics(app, analysis, symbol, canvas, body),
        Section::Why => inspector_why(app, analysis, symbol, canvas, body),
        Section::Relations => inspector_relations(app, symbol, canvas, body, active),
        Section::Clones => inspector_clones(analysis, symbol, canvas, body),
        Section::Impact => inspector_impact(analysis, symbol, canvas, body),
    }
}

/// The folder that holds the files of a symbol written in no one file, such as a module
/// that is a folder: `src/billing/`.
fn folder_of(analysis: &ProjectAnalysis, id: &SymbolId) -> String {
    let view = analysis.view();
    let paths: Vec<&str> = analysis
        .files()
        .iter()
        .filter(|file| {
            file.module
                .as_ref()
                .is_some_and(|module| view.is_within(module, id))
        })
        .map(|file| file.path.as_str())
        .collect();
    let Some(first) = paths.first() else {
        return String::new();
    };
    let mut common: Vec<&str> = first.split('/').collect();
    common.pop();
    for path in &paths {
        let parts: Vec<&str> = path.split('/').collect();
        let same = common
            .iter()
            .zip(&parts)
            .take_while(|(a, b)| a == b)
            .count();
        common.truncate(same);
    }
    if common.is_empty() {
        String::new()
    } else {
        format!("{}/", common.join("/"))
    }
}

/// The plural a scope's peers are counted in, such as `modules`.
const fn peers_name(scope: Scope) -> &'static str {
    match scope {
        Scope::Function => "functions",
        Scope::Type => "types",
        Scope::File => "files",
        Scope::Module => "modules",
        Scope::Project => "projects",
    }
}

fn inspector_metrics(
    app: &App,
    analysis: &ProjectAnalysis,
    symbol: &Symbol,
    canvas: &mut Canvas<'_>,
    body: Rect,
) {
    let subject = Subject::Symbol(symbol.id.clone());
    let Some(scope) = scope_of(symbol) else {
        return;
    };
    let peers = analysis
        .rank(&subject, MetricId::Cyclomatic)
        .map_or(0, |rank| rank.of);
    canvas.text(body.x + 1, body.y, "metric", Token::Faint, body);
    canvas.text(body.x + 15, body.y, "value", Token::Faint, body);
    let among = format!("among {} {}", grouped(peers), peers_name(scope));
    canvas.text(body.x + 21, body.y, &among, Token::Faint, body);
    canvas.text(body.x + 39, body.y, "spread", Token::Faint, body);
    let mut y = body.y + 1;
    let mut not_applicable = Vec::new();
    // The cycle has a section of its own below, which says its size.
    for metric in OVERVIEW_METRICS
        .into_iter()
        .filter(|metric| *metric != MetricId::CycleSize)
    {
        let Some(value) = analysis.value(&subject, metric) else {
            if metric.descriptor().measured_on() == Scope::Type && scope != Scope::Type {
                not_applicable.push(metric);
            }
            continue;
        };
        let shown = crate::state::shown(metric, value);
        let token = value_token(analysis, symbol, metric);
        canvas.text(body.x + 1, y, metric_label(metric), Token::Fg, body);
        canvas.text_right(body.x + 20, y, &shown, token, body);
        if let Some(rank) = analysis.rank(&subject, metric) {
            let text = format!("#{} of {}", rank.position, grouped(rank.of));
            canvas.text(body.x + 21, y, &text, Token::Fg, body);
        }
        if let Some(spread) = analysis.spread(scope, metric) {
            let glyphs = sparkline(&spread);
            canvas.text(body.x + 39, y, &glyphs, Token::Dim, body);
            if let Some(range) = analysis.spread_range(scope, metric, value) {
                let glyph: String = glyphs
                    .chars()
                    .nth(range)
                    .map(String::from)
                    .unwrap_or_default();
                let at = body.x + 39 + u16::try_from(range).unwrap_or(0);
                let heat = if metric.descriptor().heat.is_none() {
                    Token::B
                } else {
                    token
                };
                canvas.text(at, y, &glyph, heat, body);
            }
        }
        y += 1;
    }
    for metric in not_applicable {
        canvas.text(body.x + 1, y, metric_label(metric), Token::Fg, body);
        canvas.text_right(body.x + 20, y, "n/a", Token::Dim, body);
        let only = format!("{} only", peers_name(metric.descriptor().measured_on()));
        canvas.text(body.x + 21, y, &only, Token::Dim, body);
        y += 1;
    }
    y += 1;
    let rule = "─".repeat(usize::from(body.width.saturating_sub(2)));
    canvas.text(body.x + 1, y, &rule, Token::Faint, body);
    inspector_context(app, analysis, symbol, canvas, Rect { y: y + 1, ..body });
}

/// Below the metrics: the alerts inside the focus, its cycle, its relations, and keys.
fn inspector_context(
    app: &App,
    analysis: &ProjectAnalysis,
    symbol: &Symbol,
    canvas: &mut Canvas<'_>,
    body: Rect,
) {
    let mut y = body.y;
    let inside_it: Vec<_> = analysis
        .alerts()
        .iter()
        .filter(|alert| match &alert.subject {
            Subject::Symbol(id) => analysis.view().is_within(id, &symbol.id),
            Subject::File(_) => false,
        })
        .collect();
    if !inside_it.is_empty() {
        let errors = inside_it
            .iter()
            .filter(|alert| alert.severity == Severity::Error)
            .count();
        canvas.text(body.x + 1, y, "Alerts inside", Token::Faint, body);
        let x = canvas.text(body.x + 1, y + 1, &errors.to_string(), Token::H3, body);
        let x = canvas.text(x, y + 1, " error   ", Token::Fg, body);
        let x = canvas.text(
            x,
            y + 1,
            &(inside_it.len() - errors).to_string(),
            Token::H2,
            body,
        );
        let x = canvas.text(x, y + 1, " warning   ", Token::Fg, body);
        let total = format!("of {} in the project", analysis.alerts().len());
        canvas.text(x, y + 1, &total, Token::Dim, body);
        y += 3;
    }
    let module_id = if symbol.kind == SymbolKind::Module {
        Some(symbol.id.clone())
    } else {
        analysis
            .view()
            .module_of(&symbol.id)
            .map(|module| module.id.clone())
    };
    if let Some(path) = module_id.and_then(|id| analysis.cycle_path(&id)) {
        let names: Vec<String> = path
            .iter()
            .map(|(from, _, _)| {
                app.symbol(from)
                    .map_or_else(|| from.to_string(), |symbol| symbol.name.clone())
            })
            .collect();
        canvas.text(body.x + 1, y, "Cycle", Token::Faint, body);
        canvas.text(body.x + 1, y + 1, "@", Token::H2, body);
        let x = canvas.text(
            body.x + 2,
            y + 1,
            &format!(" {}   ", names.join(", ")),
            Token::Fg,
            body,
        );
        canvas.text(x, y + 1, &format!("size {}", names.len()), Token::Dim, body);
        y += 3;
    }
    let mut counts = [0_usize; 3];
    for relation in analysis.view().relations() {
        let touches = analysis.view().is_within(&relation.from, &symbol.id)
            || analysis.view().is_within(&relation.to, &symbol.id);
        if touches && let Some(count) = counts.get_mut(confidence_index(relation.confidence)) {
            *count += 1;
        }
    }
    let [exact, inferred, ambiguous] = counts;
    canvas.text(body.x + 1, y, "Relations", Token::Faint, body);
    let x = canvas.text(body.x, y + 1, &format!(" {exact} exact"), Token::Fg, body);
    let x = canvas.text(
        x + 3,
        y + 1,
        &format!("{inferred} inferred"),
        Token::Fg,
        body,
    );
    canvas.text(
        x + 3,
        y + 1,
        &format!("{ambiguous} ambiguous ~"),
        Token::H1,
        body,
    );
    let keys_y = y + 3;
    let x = canvas.text(body.x + 1, keys_y, "]a", Token::Acc, body);
    let x = canvas.text(x, keys_y, " step through its alerts   ", Token::Dim, body);
    let x = canvas.text(x, keys_y, "gr", Token::Acc, body);
    canvas.text(x, keys_y, " see its impact", Token::Dim, body);
}

const fn confidence_index(confidence: Confidence) -> usize {
    match confidence {
        Confidence::Exact => 0,
        Confidence::Inferred => 1,
        Confidence::Ambiguous => 2,
    }
}

fn inspector_why(
    app: &App,
    analysis: &ProjectAnalysis,
    symbol: &Symbol,
    canvas: &mut Canvas<'_>,
    body: Rect,
) {
    let metric = app.lens().metric;
    let value = app.shown_value(&symbol.id, metric);
    let title = format!("Why {} is {value}", metric_label(metric));
    canvas.text(body.x + 1, body.y, &title, Token::B, body);
    if let Some(reason) = analysis.why_unavailable(metric) {
        canvas.text(body.x + 1, body.y + 2, reason, Token::Dim, body);
        return;
    }
    // The value, what it means, a list, and the keys that go further, in the last row.
    let meaning = why_meaning(analysis, symbol, metric);
    for (row, line) in meaning.iter().enumerate() {
        let y = body.y + 1 + u16::try_from(row).unwrap_or(u16::MAX);
        canvas.text(body.x + 1, y, line, Token::Dim, body);
    }
    let first = body.y + 2 + u16::try_from(meaning.len()).unwrap_or(u16::MAX);
    let lines = why_lines(app, analysis, symbol, metric);
    for (row, line) in lines.iter().enumerate() {
        let y = first + u16::try_from(row).unwrap_or(u16::MAX);
        canvas.text(body.x + 1, y, line, Token::Fg, body);
    }
    let keys: &[(&str, &str)] = match metric {
        MetricId::Afferent
        | MetricId::Efferent
        | MetricId::Instability
        | MetricId::Abstractness
        | MetricId::Distance => &[("gr", "impact")],
        MetricId::CycleSize => &[("]c", "next cycle")],
        metric if shows_source(metric) => &[("gd", "opens the source")],
        _ => &[],
    };
    let mut x = body.x + 1;
    let y = body.y + body.height.saturating_sub(1);
    for (key, meaning) in keys {
        x = canvas.text(x, y, key, Token::Acc, body);
        x = canvas.text(x + 1, y, meaning, Token::Dim, body) + 3;
    }
}

/// What a value of a metric with no lines to point at is, in a line or two.
fn why_meaning(analysis: &ProjectAnalysis, symbol: &Symbol, metric: MetricId) -> Vec<String> {
    let name = &symbol.name;
    match metric {
        MetricId::Lcom => {
            let groups = analysis.method_groups(&symbol.id).unwrap_or_default().len();
            vec![
                format!("These {groups} groups of methods share no fields."),
                "Each group could become its own type.".to_owned(),
            ]
        }
        MetricId::Members => vec!["Its fields, then its methods.".to_owned()],
        MetricId::Afferent
        | MetricId::Efferent
        | MetricId::Instability
        | MetricId::Abstractness
        | MetricId::Distance => vec![format!("The modules on each side of {name}.")],
        MetricId::CycleSize => vec![
            "The way around the cycle.".to_owned(),
            "Removing one link breaks it.".to_owned(),
        ],
        MetricId::Duplication => vec![format!("Where the copies of {name} are.")],
        _ => Vec::new(),
    }
}

fn inspector_relations(
    app: &App,
    symbol: &Symbol,
    canvas: &mut Canvas<'_>,
    body: Rect,
    active: bool,
) {
    let relations = app.relations_of(&symbol.id);
    if relations.is_empty() {
        canvas.text(body.x + 1, body.y, "No relations.", Token::Dim, body);
        return;
    }
    let mut y = body.y;
    let mut heading = None;
    for (index, (other, kind, confidence, incoming)) in relations.iter().enumerate() {
        let title = if *incoming { "Used by" } else { "Uses" };
        if heading != Some(title) {
            if heading.is_some() {
                y += 1;
            }
            canvas.text(body.x + 1, y, title, Token::B, body);
            y += 1;
            heading = Some(title);
        }
        let line = Rect {
            y,
            height: 1,
            ..body
        };
        let guess = if *confidence == Confidence::Exact {
            ""
        } else {
            " ~"
        };
        canvas.text(body.x + 1, y, kind.name(), Token::Dim, line);
        canvas.text(body.x + 12, y, other.as_str(), Token::Fg, line);
        let sure = format!("{}{guess}", confidence.name());
        canvas.text_right(body.x + body.width - 1, y, &sure, Token::Dim, line);
        if active && index == app.inspector_row() {
            canvas.paint(line, Token::Sel);
        }
        y += 1;
    }
}

fn inspector_impact(
    analysis: &ProjectAnalysis,
    symbol: &Symbol,
    canvas: &mut Canvas<'_>,
    body: Rect,
) {
    let x = body.x + 1;
    let impact = analysis.impact(&symbol.id);
    let all: Vec<&(SymbolId, Confidence)> = impact.all().collect();
    if all.is_empty() {
        canvas.text(x, body.y, "Nothing depends on this.", Token::Dim, body);
        return;
    }
    canvas.text(
        x,
        body.y,
        &format!("If {} changes", symbol.name),
        Token::Fg,
        body,
    );
    let widest = impact.levels.iter().map(Vec::len).max().unwrap_or(1).max(1);
    let mut y = body.y + 1;
    for (distance, level) in impact.levels.iter().enumerate() {
        canvas.text(x, y, &format!("distance {}", distance + 1), Token::Fg, body);
        canvas.text_right(x + 15, y, &level.len().to_string(), Token::Fg, body);
        let share = f64::from(u32::try_from(level.len()).unwrap_or(u32::MAX));
        let (filled, rest) = bar(share, f64::from(u32::try_from(widest).unwrap_or(1)), 15);
        let end = canvas.text(x + 16, y, &filled, Token::Acc, body);
        canvas.text(end, y, &rest, Token::Track, body);
        y += 1;
    }
    let reach = analysis.impact_reach(&impact);
    y += 1;
    let modules = format!("{} of {} modules", reach.modules, grouped(reach.of_modules));
    canvas.text(x, y, &modules, Token::Fg, body);
    let packages = format!("{} of {} packages", reach.packages, reach.of_packages);
    canvas.text(x, y + 1, &packages, Token::Fg, body);
    y += 3;
    canvas.text(x, y, "How sure", Token::Faint, body);
    let mut guesses = 0;
    for (offset, confidence) in [
        Confidence::Exact,
        Confidence::Inferred,
        Confidence::Ambiguous,
    ]
    .into_iter()
    .enumerate()
    {
        let count = all.iter().filter(|(_, sure)| *sure == confidence).count();
        if confidence != Confidence::Exact {
            guesses += count;
        }
        let row = y + 1 + u16::try_from(offset).unwrap_or(0);
        canvas.text(x, row, confidence.name(), Token::Fg, body);
        canvas.text_right(x + 15, row, &count.to_string(), Token::Fg, body);
    }
    y += 4;
    let sure = match guesses {
        0 => "All of these are exact.".to_owned(),
        1 => "1 of these is a guess.".to_owned(),
        many => format!("{many} of these are guesses."),
    };
    canvas.text(x, y, &sure, Token::Dim, body);
    if guesses > 0 {
        let end = canvas.text(x, y + 1, "Space p", Token::Acc, body);
        canvas.text(end, y + 1, " settles them", Token::Dim, body);
    }
    y += 3;
    canvas.text(x, y, "Nearest dependents", Token::Faint, body);
    let right = body.x + body.width.saturating_sub(4);
    for (row, (id, confidence)) in impact.levels.iter().flatten().enumerate() {
        let line_y = y + 1 + u16::try_from(row).unwrap_or(u16::MAX);
        if line_y >= body.y + body.height {
            break;
        }
        let guess = if *confidence == Confidence::Exact {
            ""
        } else {
            " ~"
        };
        let name = format!("{}{guess}", short_subject(&Subject::Symbol(id.clone())));
        canvas.text(x, line_y, &name, Token::Fg, body);
        canvas.text_right(right, line_y, confidence.name(), Token::Dim, body);
    }
}

fn overview(app: &App, canvas: &mut Canvas<'_>, body: Rect) {
    let narrow = body.width < SIDE_BY_SIDE;
    let left_width = if narrow { 0 } else { 40 };
    if !narrow {
        overview_left(
            app,
            canvas,
            Rect {
                width: left_width,
                ..body
            },
        );
    }
    let gap = u16::from(!narrow);
    let right = Rect {
        x: body.x + left_width + gap,
        width: body.width - left_width - gap,
        ..body
    };
    let metrics_height = 18.min(right.height / 2 + 2);
    let metrics_area = Rect {
        height: metrics_height,
        ..right
    };
    let ranking_area = Rect {
        y: right.y + metrics_height,
        height: right.height - metrics_height,
        ..right
    };
    let active = app.pane();
    overview_metrics(app, canvas, metrics_area, active == Some(Pane::Metrics));
    overview_ranking(app, canvas, ranking_area, active == Some(Pane::Ranking));
}

fn overview_left(app: &App, canvas: &mut Canvas<'_>, area: Rect) {
    let project = app
        .focus()
        .and_then(|focus| app.symbol(focus))
        .map_or_else(|| "project".to_owned(), |symbol| root_name(app, symbol));
    let summary = Rect {
        height: 12.min(area.height),
        ..area
    };
    summary_pane(app, canvas, summary, &project);
    let alerts = Rect {
        y: area.y + summary.height,
        height: 8.min(area.height.saturating_sub(summary.height)),
        ..area
    };
    alerts_pane(app, canvas, alerts);
    let analysis = Rect {
        y: alerts.y + alerts.height,
        height: area.height.saturating_sub(summary.height + alerts.height),
        ..area
    };
    analysis_pane(app, canvas, analysis);
}

fn summary_pane(app: &App, canvas: &mut Canvas<'_>, area: Rect, project: &str) {
    let inside = canvas.pane(area, project, false);
    let label_x = inside.x + 1;
    let value_end = inside.x + 22;
    let Some(analysis) = app.analysis() else {
        let files = app.running_analysis().map_or(0, |running| running.files.1);
        let rows: [(&str, String); 6] = [
            ("Packages", "...".to_owned()),
            ("Modules", "...".to_owned()),
            ("Types", "...".to_owned()),
            ("Functions", "...".to_owned()),
            ("Files", grouped(files)),
            ("Lines", "...".to_owned()),
        ];
        for (index, (label, value)) in rows.iter().enumerate() {
            let y = inside.y + u16::try_from(index).unwrap_or_default();
            canvas.text(label_x, y, label, Token::Fg, inside);
            canvas.text_right(value_end, y, value, Token::Fg, inside);
        }
        return;
    };
    let view = analysis.view();
    let count = |kinds: &[SymbolKind]| {
        view.symbols()
            .filter(|symbol| kinds.contains(&symbol.kind))
            .count()
    };
    let types = view
        .symbols()
        .filter(|symbol| symbol.kind.is_type())
        .count();
    let tests = analysis.tests();
    let counted: Vec<_> = analysis
        .files()
        .iter()
        .filter(|file| tests.shows(file.role))
        .collect();
    let (files, lines) = analysis.counted_size();
    let rows = [
        ("Packages", count(&[SymbolKind::Package])),
        ("Modules", count(&[SymbolKind::Module])),
        ("Types", types),
        (
            "Functions",
            count(&[SymbolKind::Function, SymbolKind::Method]),
        ),
        ("Files", files),
        ("Lines", usize::try_from(lines).unwrap_or_default()),
    ];
    for (index, (label, value)) in rows.iter().enumerate() {
        let y = inside.y + u16::try_from(index).unwrap_or_default();
        canvas.text(label_x, y, label, Token::Fg, inside);
        canvas.text_right(value_end, y, &grouped(*value), Token::Fg, inside);
    }
    let note = if app.lens().tests {
        "tests counted"
    } else {
        "tests left out"
    };
    canvas.text(inside.x + 24, inside.y + 5, note, Token::Faint, inside);
    let mut by_language: Vec<(&'static str, u32)> = Vec::new();
    for file in &counted {
        let name = file.language.name();
        let code = file.counted_lines(tests).code;
        match by_language
            .iter_mut()
            .find(|(language, _)| *language == name)
        {
            Some((_, total)) => *total += code,
            None => by_language.push((name, code)),
        }
    }
    by_language.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    for (index, (language, code)) in by_language.iter().take(2).enumerate() {
        let y = inside.y + 7 + u16::try_from(index).unwrap_or_default();
        let share = if lines > 0 {
            f64::from(*code) / f64::from(lines)
        } else {
            0.0
        };
        let (filled, rest) = bar(share, 1.0, 18);
        canvas.text(label_x, y, language, Token::Fg, inside);
        let x = canvas.text(inside.x + 12, y, &filled, Token::Acc, inside);
        canvas.text(x, y, &rest, Token::Track, inside);
        let percent = format!("{}%", steps(share, 100));
        canvas.text_right(inside.x + 35, y, &percent, Token::Dim, inside);
    }
}

fn alerts_pane(app: &App, canvas: &mut Canvas<'_>, area: Rect) {
    let inside = canvas.pane(area, "Alerts", false);
    let Some(analysis) = app.analysis() else {
        canvas.text(
            inside.x + 1,
            inside.y,
            "Waiting for analysis.",
            Token::Dim,
            inside,
        );
        return;
    };
    let rules: Vec<_> = analysis
        .rules()
        .iter()
        .filter(|rule| rule.enabled)
        .collect();
    if rules.is_empty() {
        canvas.text(inside.x + 1, inside.y, "No alert rules", Token::Dim, inside);
        canvas.text(
            inside.x + 1,
            inside.y + 1,
            "A adds one from here",
            Token::Dim,
            inside,
        );
        return;
    }
    let errors = analysis
        .alerts()
        .iter()
        .filter(|alert| alert.severity == Severity::Error)
        .count();
    let warnings = analysis.alerts().len() - errors;
    let x = canvas.text(
        inside.x + 1,
        inside.y,
        &errors.to_string(),
        Token::H3,
        inside,
    );
    let x = canvas.text(x, inside.y, " error   ", Token::Fg, inside);
    let x = canvas.text(x, inside.y, &warnings.to_string(), Token::H2, inside);
    canvas.text(x, inside.y, " warning", Token::Fg, inside);
    for (index, rule) in rules.iter().enumerate() {
        let y = inside.y + 2 + u16::try_from(index).unwrap_or(u16::MAX);
        let hits = analysis
            .alerts()
            .iter()
            .filter(|alert| alert.metric == rule.metric && alert.rule == rule.name)
            .count();
        let (mark, token) = if rule.severity == Severity::Error {
            ("!!", Token::H3)
        } else {
            ("!", Token::H2)
        };
        canvas.text(inside.x + 1, y, mark, token, inside);
        canvas.text(inside.x + 5, y, &hits.to_string(), Token::Fg, inside);
        canvas.text(inside.x + 9, y, &rule.title(), Token::Fg, inside);
    }
}

/// What the analysis is doing now, or how far to trust what it found.
fn analysis_pane(app: &App, canvas: &mut Canvas<'_>, area: Rect) {
    let inside = canvas.pane(area, "Analysis", false);
    let x = inside.x + 1;
    let line = |index: u16| inside.y + index;
    let pair = |canvas: &mut Canvas<'_>, index: u16, label: &str, value: &str, token: Token| {
        let end = canvas.text(x, line(index), label, Token::Dim, inside);
        canvas.text(end, line(index), value, token, inside);
    };
    if let Some(running) = app.running_analysis().filter(|running| !running.precise) {
        running_lines(app, running, canvas, inside);
        mascot(app, canvas, inside);
        return;
    }
    let syntax = if app.stale() { "stale" } else { "fresh" };
    pair(canvas, 0, "Syntax  ", syntax, Token::Fg);
    if let Some(running) = app.running_analysis() {
        let label = running
            .percent
            .map_or_else(|| "running".to_owned(), |percent| format!("{percent}%"));
        pair(canvas, 1, "Precise ", &label, Token::Accb);
        // With a percentage, how far it has got; without, how long it has run.
        if let Some(percent) = running.percent {
            let (filled, rest) = bar(f64::from(percent), 100.0, 17);
            let end = canvas.text(x, line(2), &filled, Token::Acc, inside);
            canvas.text(end, line(2), &rest, Token::Track, inside);
        } else {
            let since = format!("for {}", elapsed(app.now().saturating_sub(running.since)));
            canvas.text(x, line(2), &since, Token::Faint, inside);
        }
        let indexer = running.indexer.as_deref().unwrap_or("the indexer");
        canvas.text(x, line(3), indexer, Token::Faint, inside);
    } else {
        let precise = if app.precise_layer() {
            "indexed"
        } else {
            "no index"
        };
        pair(canvas, 1, "Precise ", precise, Token::Fg);
    }
    if let Some(analysis) = app.analysis() {
        let relations: Vec<_> = analysis.view().relations().collect();
        let exact = relations
            .iter()
            .filter(|relation| relation.confidence == Confidence::Exact)
            .count();
        let share = if relations.is_empty() {
            "n/a".to_owned()
        } else {
            let share = steps(
                f64::from(u32::try_from(exact).unwrap_or(u32::MAX))
                    / f64::from(u32::try_from(relations.len()).unwrap_or(u32::MAX)),
                100,
            );
            format!("{share}%")
        };
        pair(canvas, 4, "Exact   ", &share, Token::Fg);
        pair(
            canvas,
            5,
            "Problems ",
            &analysis.problems().len().to_string(),
            Token::Fg,
        );
        let tests = if app.lens().tests {
            "counted"
        } else {
            "left out"
        };
        pair(canvas, 6, "Tests   ", tests, Token::Fg);
        if analysis
            .problems()
            .contains(&cyclops_domain::problem::Problem::NoSourceFiles)
        {
            let text = cyclops_domain::problem::Problem::NoSourceFiles.to_string();
            canvas.text(x, line(7), &text, Token::Dim, inside);
        }
    }
    if let Some(failure) = app.failure() {
        canvas.text(x, line(7), failure, Token::H2, inside);
    }
    let end = canvas.text(x, line(8), "Space s", Token::Acc, inside);
    canvas.text(end, line(8), " details", Token::Dim, inside);
    if app.running_analysis().is_some() {
        canvas.text(x, line(9), "Keep browsing.", Token::Dim, inside);
    } else {
        let end = canvas.text(x, line(9), "Space p", Token::Acc, inside);
        canvas.text(end, line(9), " precise", Token::Dim, inside);
    }
    mascot(app, canvas, inside);
}

fn overview_metrics(app: &App, canvas: &mut Canvas<'_>, area: Rect, active: bool) {
    let inside = canvas.pane(area, "Metrics", active);
    if let Some(failure) = app.failure() {
        canvas.text(inside.x + 1, inside.y, failure, Token::H2, inside);
        return;
    }
    let Some(analysis) = app.analysis() else {
        partial_metrics(app, canvas, inside);
        return;
    };
    let value_end = inside.x + inside.width - 1;
    canvas.text(inside.x + 1, inside.y, "metric", Token::Faint, inside);
    canvas.text(inside.x + 15, inside.y, "spread", Token::Faint, inside);
    canvas.text(inside.x + 25, inside.y, "highest", Token::Faint, inside);
    canvas.text_right(value_end, inside.y, "value", Token::Faint, inside);
    for (index, metric) in OVERVIEW_METRICS.iter().enumerate() {
        let y = inside.y + 1 + u16::try_from(index).unwrap_or_default();
        if y >= inside.y + inside.height {
            break;
        }
        let line = Rect {
            y,
            height: 1,
            ..inside
        };
        if *metric == app.lens().metric {
            canvas.fill(line, Token::Sel);
        }
        canvas.hit(line, Target::Metric(*metric));
        canvas.text(inside.x + 1, y, metric_label(*metric), Token::Fg, line);
        if let Some(reason) = analysis.why_unavailable(*metric) {
            canvas.text(inside.x + 25, y, reason, Token::Dim, line);
            continue;
        }
        let scope = metric.descriptor().measured_on();
        let worst = analysis.worst(*metric);
        let heat = worst
            .as_ref()
            .and_then(|worst| metric.descriptor().heat_of(scope, worst.value));
        let token = if metric.descriptor().heat.is_none() {
            Token::B
        } else {
            heat_token(heat)
        };
        if let Some(spread) = analysis.spread(scope, *metric) {
            let glyphs = sparkline(&spread);
            canvas.text(inside.x + 15, y, &glyphs, Token::Dim, line);
            if let Some(last) = spread.iter().rposition(|count| *count > 0) {
                let glyph: String = glyphs
                    .chars()
                    .nth(last)
                    .map(String::from)
                    .unwrap_or_default();
                let at = inside.x + 15 + u16::try_from(last).unwrap_or(0);
                canvas.text(at, y, &glyph, token, line);
            }
        }
        match worst {
            Some(worst) => {
                let names: Vec<String> = worst.subjects.iter().take(3).map(short_subject).collect();
                let value = crate::state::shown(*metric, worst.value);
                // The names may run up to a gap before the widest value.
                let room = Rect {
                    width: inside.width.saturating_sub(8),
                    ..line
                };
                canvas.text(inside.x + 25, y, &names.join(", "), Token::Fg, room);
                let value_token = if metric.descriptor().heat.is_none() {
                    Token::Fg
                } else {
                    token
                };
                canvas.text_right(value_end, y, &value, value_token, line);
            }
            None => {
                canvas.text_right(value_end, y, "n/a", Token::Dim, line);
            }
        }
    }
}

/// The last two parts of a subject, as the Overview names it.
fn short_subject(subject: &Subject) -> String {
    match subject {
        Subject::File(path) => path.to_string(),
        Subject::Symbol(id) => {
            let parts: Vec<&str> = id.as_str().split("::").collect();
            let keep = parts.len().saturating_sub(2);
            parts
                .get(keep..)
                .map(|parts| parts.join("::"))
                .unwrap_or_default()
        }
    }
}

fn overview_ranking(app: &App, canvas: &mut Canvas<'_>, area: Rect, active: bool) {
    let metric = app.lens().metric;
    let partial = app
        .running_analysis()
        .and_then(|running| running.partial.as_ref());
    let title = if app.analysis().is_none() && partial.is_some() {
        format!("Highest by {}, so far", metric_label(metric))
    } else {
        format!("Highest by {}", metric_label(metric))
    };
    let inside = canvas.pane(area, &title, active);
    if !app.has_settings() {
        let y = inside.y + inside.height.saturating_sub(2);
        let note = "No project settings yet: defaults, local cache only.";
        canvas.text(inside.x + 1, y, note, Token::Dim, inside);
        canvas.text(
            inside.x + 1,
            y + 1,
            ":init saves settings for the team.",
            Token::Dim,
            inside,
        );
    }
    let Some(analysis) = app.analysis() else {
        if let Some(partial) = partial {
            partial_ranking(canvas, inside, partial, metric);
        }
        return;
    };
    let on_files = metric.descriptor().measured_on() == Scope::File;
    let right = inside.x + inside.width;
    ranking_header(canvas, inside, metric, on_files);
    let ranking = app.ranking();
    let highest = ranking
        .first()
        .and_then(|subject| analysis.value(subject, metric))
        .unwrap_or(1.0);
    for (index, subject) in ranking.iter().enumerate() {
        let y = inside.y + 1 + u16::try_from(index).unwrap_or_default();
        let line = Rect {
            y,
            height: 1,
            ..inside
        };
        let value = analysis.value(subject, metric);
        let shown = value.map_or_else(
            || "n/a".to_owned(),
            |value| crate::state::shown(metric, value),
        );
        let heat = value.and_then(|value| {
            metric
                .descriptor()
                .heat_of(metric.descriptor().measured_on(), value)
        });
        let token = heat_token(heat);
        if on_files {
            // The path stops short of the churn column.
            let path = fit_start(&subject.to_string(), 35);
            canvas.text(inside.x + 1, y, &path, Token::Fg, line);
            let churn = analysis
                .value(subject, MetricId::Churn)
                .map_or_else(String::new, |value| {
                    crate::state::shown(MetricId::Churn, value)
                });
            let complexity = analysis
                .value(subject, MetricId::Cyclomatic)
                .map_or_else(String::new, |value| {
                    crate::state::shown(MetricId::Cyclomatic, value)
                });
            canvas.text_right(inside.x + 42, y, &churn, Token::Fg, line);
            canvas.text_right(inside.x + 49, y, &complexity, Token::Fg, line);
            canvas.text_right(inside.x + 57, y, &shown, token, line);
            if let Some(value) = value {
                let (filled, rest) = bar(value, highest, 17);
                let x = canvas.text(inside.x + 59, y, &filled, token, line);
                canvas.text(x, y, &rest, Token::Track, line);
            }
        } else {
            let kind = match subject {
                Subject::Symbol(id) => app.symbol(id).map_or("", |symbol| symbol.kind.name()),
                Subject::File(_) => "file",
            };
            let name = fit_start(&subject.to_string(), 51);
            canvas.text(inside.x + 1, y, &name, Token::Fg, line);
            canvas.text(inside.x + 53, y, kind, Token::Fg, line);
            canvas.text_right(right - 3, y, &shown, token, line);
        }
        if active && index == app.ranking_row() {
            canvas.paint(line, Token::Sel);
        }
        if let Subject::Symbol(id) = subject {
            canvas.hit(line, Target::Symbol(id.clone()));
        }
    }
    if app.has_settings() {
        ranking_footer(analysis, canvas, inside, metric);
    }
}

/// The column titles of the ranking.
fn ranking_header(canvas: &mut Canvas<'_>, inside: Rect, metric: MetricId, on_files: bool) {
    let right = inside.x + inside.width;
    if on_files {
        canvas.text(inside.x + 1, inside.y, "file", Token::Faint, inside);
        canvas.text(inside.x + 37, inside.y, "churn", Token::Faint, inside);
        canvas.text(inside.x + 47, inside.y, "cx", Token::Faint, inside);
        canvas.text(
            inside.x + 50,
            inside.y,
            metric_label(metric),
            Token::Faint,
            inside,
        );
    } else {
        canvas.text(inside.x + 1, inside.y, "symbol", Token::Faint, inside);
        canvas.text(inside.x + 53, inside.y, "kind", Token::Faint, inside);
        canvas.text_right(
            right - 3,
            inside.y,
            metric_label(metric),
            Token::Faint,
            inside,
        );
    }
}

/// The note under the ranking, and how to see all of it.
fn ranking_footer(
    analysis: &ProjectAnalysis,
    canvas: &mut Canvas<'_>,
    inside: Rect,
    metric: MetricId,
) {
    let y = inside.y + inside.height.saturating_sub(2);
    let count = analysis.ranking(metric, None, usize::MAX).len();
    canvas.text(
        inside.x + 1,
        y,
        "This list follows the selected metric.",
        Token::Dim,
        inside,
    );
    let end = canvas.text(inside.x + 42, y, "Space f", Token::Acc, inside);
    canvas.text(
        end,
        y,
        &format!(" rank all {}", grouped(count)),
        Token::Dim,
        inside,
    );
}

fn diagram(app: &App, canvas: &mut Canvas<'_>, area: Rect, active: bool) {
    let focus_name = app.focus().and_then(|focus| app.symbol(focus)).map_or_else(
        || (String::new(), SymbolKind::Package),
        |symbol| (symbol.name.clone(), symbol.kind),
    );
    let Some((drawing, scene)) = app.scene() else {
        canvas.pane(area, "Diagram", active);
        return;
    };
    let title = match (drawing.level, focus_name.1) {
        (Level::Module, SymbolKind::Package) => format!("Modules of {}", focus_name.0),
        (Level::Package, SymbolKind::Project) => format!("Packages of {}", focus_name.0),
        (_, kind)
            if drawing.depth == 0 && !matches!(kind, SymbolKind::Package | SymbolKind::Project) =>
        {
            format!("Inside {}", focus_name.0)
        }
        _ => format!("Neighbors of {}", focus_name.0),
    };
    let inside = canvas.pane(area, &title, active);
    let level = match drawing.level {
        Level::Project => "project",
        Level::Package => "package",
        Level::Module => "module",
        Level::Type => "type",
        Level::Member => "member",
    };
    let relations = if app.diagram_relations().is_empty() {
        "all".to_owned()
    } else {
        app.diagram_relations()
            .iter()
            .map(|kind| kind.name())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut controls = format!(
        "level {level}   depth {}   relations {relations}   overlay {}",
        drawing.depth,
        app.metric_label()
    );
    if app.lens().filter.is_some() {
        let all = app.unfiltered_nodes();
        controls = format!("{controls}   {} of {all} nodes", drawing.view.nodes.len());
    }
    canvas.text(inside.x + 1, inside.y, &controls, Token::Dim, inside);
    for route in routes(&scene, &drawing.view.edges) {
        draw_route(canvas, &route, inside);
    }
    let selected = app.selected();
    let metric = app.lens().metric;
    let marks = app.diagram_marks();
    for card in &scene.cards {
        draw_card(app, canvas, card, selected.as_ref(), metric, &marks);
    }
    for more in &scene.more {
        canvas.frame(more.area, Token::Line, ['╭', '╮', '╰', '╯', '─', '│']);
        let label = format!("+{} more", more.hidden.len());
        canvas.text(
            more.area.x + 2,
            more.area.y + 1,
            &label,
            Token::Faint,
            more.area,
        );
        canvas.hit(more.area, Target::More(more.column));
    }
    if drawing.level != Level::Type {
        column_labels(app, canvas, &scene, &focus_name.0, inside);
    }
    let legend = inside.y + inside.height.saturating_sub(3);
    let marks_row = if drawing.level == Level::Type {
        "+ public  # restricted  - private   ! alert  @ module in a cycle"
    } else {
        "! alert   @ in a cycle   Numbers are the overlay metric."
    };
    let rows = [
        "────── exact  ╌╌╌╌╌╌ inferred  ┄┄┄┄┄┄ ambiguous  ━━━━━━ cycle",
        "> uses, imports, calls  ▷ inherits  ○ implements  ◆ composes",
        marks_row,
    ];
    for (offset, row) in rows.iter().enumerate() {
        let y = legend + u16::try_from(offset).unwrap_or(0);
        canvas.text(inside.x + 1, y, row, Token::Dim, inside);
    }
}

/// Above each column of cards, how it stands to the focus.
fn column_labels(
    app: &App,
    canvas: &mut Canvas<'_>,
    scene: &crate::scene::Scene,
    focus: &str,
    inside: Rect,
) {
    let Some(focus_column) = app
        .focus()
        .and_then(|id| scene.card(id))
        .map(|card| card.column)
    else {
        return;
    };
    let mut columns: Vec<(usize, u16, u16)> = scene
        .cards
        .iter()
        .map(|card| (card.column, card.area.x, card.area.y))
        .chain(
            scene
                .more
                .iter()
                .map(|more| (more.column, more.area.x, more.area.y)),
        )
        .collect();
    columns.sort_unstable();
    columns.dedup_by_key(|(column, ..)| *column);
    let top = columns.iter().map(|(.., y)| *y).min().unwrap_or(inside.y);
    for (column, x, _) in columns {
        let label = match column.cmp(&focus_column) {
            std::cmp::Ordering::Less => format!("depends on {focus}"),
            std::cmp::Ordering::Equal => "focus".to_owned(),
            std::cmp::Ordering::Greater => format!("{focus} depends on"),
        };
        canvas.text(x, top.saturating_sub(1), &label, Token::Faint, inside);
    }
}

fn map(app: &App, canvas: &mut Canvas<'_>, area: Rect, active: bool) {
    let frames_area = Rect {
        height: area.height.saturating_sub(1),
        ..area
    };
    let scene = app.map_scene(frames_area);
    let marks = app.tree_marks();
    if let Some(title) = &scene.title {
        canvas.pane(frames_area, title, active);
    }
    for frame in &scene.frames {
        canvas.pane(frame.area, &frame.title, active);
    }
    for group in &scene.groups {
        let mark = marks
            .get(&group.id)
            .map_or_else(String::new, |mark| format!(" {mark}"));
        let label = format!("{}{mark}", group.name);
        canvas.text(group.area.x + 1, group.area.y, &label, Token::B, group.area);
    }
    let selected = app.selected();
    for (index, tile) in scene.tiles.iter().enumerate() {
        if tile.folded.is_empty() {
            map_tile(app, canvas, tile, &marks, selected.as_ref());
        } else {
            small_tile(canvas, tile, index, selected.as_ref());
        }
    }
    map_legend(app, canvas, area, &scene);
}

/// One tile: colored by the metric, with its name, marks and value.
fn map_tile(
    app: &App,
    canvas: &mut Canvas<'_>,
    tile: &MapTile,
    marks: &HashMap<SymbolId, String>,
    selected: Option<&SymbolId>,
) {
    let Some(analysis) = app.analysis() else {
        return;
    };
    let Some(symbol) = app.symbol(&tile.id) else {
        return;
    };
    let metric = app.lens().metric;
    let area = tile.area;
    let token = match value_token(analysis, symbol, metric) {
        Token::H1 => Token::T2,
        Token::H2 => Token::T3,
        Token::H3 => Token::T4,
        Token::H0 => Token::T1,
        _ => Token::T0,
    };
    canvas.fill(area, token);
    let mark = marks
        .get(&tile.id)
        .map_or_else(String::new, |mark| format!(" {mark}"));
    if selected == Some(&tile.id) {
        let label = format!("▸ {}{mark} ", tile.name);
        canvas.text(area.x + 1, area.y, &label, Token::On, area);
    } else {
        let label = format!("{}{mark}", tile.name);
        canvas.text(area.x + 1, area.y, &label, token, area);
    }
    let value = app.shown_value(&tile.id, metric);
    canvas.text(area.x + 1, area.y + 1, &value, token, area);
    canvas.hit(area, Target::Symbol(tile.id.clone()));
}

/// The `+N small` tile, which stands for the tiles too small to show.
fn small_tile(canvas: &mut Canvas<'_>, tile: &MapTile, index: usize, selected: Option<&SymbolId>) {
    let area = tile.area;
    canvas.fill(area, Token::T0);
    if selected == Some(&tile.id) {
        let label = format!("▸ {} ", tile.name);
        canvas.text(area.x + 1, area.y, &label, Token::On, area);
    } else {
        canvas.text(area.x + 1, area.y, &tile.name, Token::T0, area);
    }
    canvas.hit(area, Target::More(index));
}

/// The row below the map: what area and color say, how much a filter left, and the
/// size of the selected tile.
fn map_legend(app: &App, canvas: &mut Canvas<'_>, area: Rect, scene: &MapScene) {
    let y = area.y + area.height.saturating_sub(1);
    let row = Rect {
        y,
        height: 1,
        ..area
    };
    let end = canvas.text(area.x + 1, y, "area ", Token::Faint, row);
    let end = canvas.text(end, y, "lines", Token::Fg, row);
    let end = canvas.text(end + 3, y, "color ", Token::Faint, row);
    let mut end = canvas.text(end, y, app.metric_label(), Token::Fg, row) + 3;
    for (label, token) in [
        ("n/a", Token::T0),
        ("quiet", Token::T1),
        ("mild", Token::T2),
        ("high", Token::T3),
        ("severe", Token::T4),
    ] {
        end = canvas.text(end, y, &format!(" {label} "), token, row);
    }
    if app.lens().filter.is_some() {
        let left = format!("{} of {} tiles", scene.tile_count(), app.unfiltered_tiles());
        end = canvas.text(end + 2, y, &left, Token::Dim, row);
    }
    let selected = app.selected();
    if let Some(tile) = scene
        .tiles
        .iter()
        .find(|tile| Some(&tile.id) == selected.as_ref())
    {
        let lines = crate::state::shown(MetricId::Lines, tile.lines);
        let size = format!("{}: {lines} lines", tile.name);
        let right = area.x + area.width - 1;
        if right.saturating_sub(width(&size)) > end + 1 {
            canvas.text_right(right, y, &size, Token::Faint, row);
        }
    }
}

fn find(app: &App, canvas: &mut Canvas<'_>, area: Rect, active: bool) {
    let filter_area = Rect {
        height: 3.min(area.height),
        ..area
    };
    let prompt = app
        .prompt()
        .filter(|prompt| prompt.kind == PromptKind::Filter);
    let bar = canvas.pane(filter_area, "Filter", prompt.is_some());
    let typed = prompt.map_or_else(
        || {
            app.lens()
                .filter
                .as_ref()
                .map_or("", |filter| filter.text.as_str())
        },
        |prompt| prompt.text.as_str(),
    );
    let end = canvas.text(bar.x + 1, bar.y, "> ", Token::Acc, bar);
    let end = canvas.text(end, bar.y, typed, Token::Fg, bar);
    if let Some(prompt) = prompt {
        canvas.text(end, bar.y, " ", Token::On, bar);
        if let Some(error) = &prompt.error {
            canvas.text_right(bar.x + bar.width - 1, bar.y, error, Token::H2, bar);
        }
    }
    let found = app.found();
    let (sort, lowest_first) = app.sorted_by();
    let order = if lowest_first { "asc" } else { "desc" };
    let scopes: Vec<Scope> = found.iter().filter_map(|symbol| scope_of(symbol)).collect();
    let (total, peers) = match scopes.first() {
        Some(scope) if scopes.iter().all(|other| other == scope) => (
            app.analysis().map_or(0, |analysis| {
                analysis
                    .view()
                    .symbols()
                    .filter(|symbol| scope_of(symbol) == Some(*scope))
                    .count()
            }),
            peers_name(*scope),
        ),
        _ => (
            app.analysis()
                .map_or(0, |analysis| analysis.view().symbols().count()),
            "symbols",
        ),
    };
    let summary = format!(
        "{} of {} {peers}   sorted by {} {order}",
        grouped(found.len()),
        grouped(total),
        metric_label(sort)
    );
    canvas.text(area.x + 1, area.y + 3, &summary, Token::Dim, area);
    let results = Rect {
        y: area.y + 4,
        height: area.height.saturating_sub(4),
        ..area
    };
    let inside = canvas.pane(results, "Results", active);
    find_results(app, canvas, inside, sort);
    let rule = "─".repeat(usize::from(inside.width.saturating_sub(2)));
    let bottom = inside.y + inside.height.saturating_sub(3);
    canvas.text(inside.x + 1, bottom, &rule, Token::Faint, inside);
    let end = canvas.text(
        inside.x + 1,
        bottom + 1,
        "Same query in a script   ",
        Token::Faint,
        inside,
    );
    let end = canvas.text(end, bottom + 1, "yc", Token::Acc, inside);
    canvas.text(end, bottom + 1, " copy", Token::Dim, inside);
    canvas.text(
        inside.x + 1,
        bottom + 2,
        &app.query_command(),
        Token::Fg,
        inside,
    );
    if let Some(prompt) = prompt {
        completion_popup(app, canvas, bar, &prompt.text);
    }
}

/// The columns of Find, after the symbol and its kind.
const FIND_COLUMNS: [MetricId; 4] = [
    MetricId::Cyclomatic,
    MetricId::Cognitive,
    MetricId::Lines,
    MetricId::Churn,
];

/// The symbols that pass the filter, worst first, with a column for each metric.
fn find_results(app: &App, canvas: &mut Canvas<'_>, inside: Rect, sort: MetricId) {
    let mut columns: Vec<MetricId> = FIND_COLUMNS.to_vec();
    if !columns.contains(&sort) {
        columns.push(sort);
    }
    // The symbol takes up to 53 columns, and the kind 10, then each metric its heading
    // after a gap of two. The sorted one is marked with the way it runs.
    let (_, lowest_first) = app.sorted_by();
    let labels: Vec<String> = columns
        .iter()
        .map(|metric| {
            let arrow = match (*metric == sort, lowest_first) {
                (false, _) => "",
                (true, false) => "▾",
                (true, true) => "▴",
            };
            format!("{}{arrow}", metric_label(*metric))
        })
        .collect();
    let widths: u16 = labels.iter().map(|label| width(label) + 2).sum();
    let kind_x = inside.x + 4 + 53.min(inside.width.saturating_sub(4 + 10 + widths));
    let mut edges = Vec::new();
    let mut right = kind_x + 8;
    for label in &labels {
        right += 2 + width(label);
        edges.push(right);
    }
    let heading = inside.y;
    canvas.text(inside.x + 4, heading, "symbol", Token::Faint, inside);
    canvas.text(kind_x, heading, "kind", Token::Faint, inside);
    for ((metric, edge), label) in columns.iter().zip(&edges).zip(&labels) {
        let token = if *metric == sort {
            Token::Accb
        } else {
            Token::Faint
        };
        canvas.text_right(*edge, heading, label, token, inside);
        let start = edge.saturating_sub(width(label));
        canvas.hit(
            Rect::new(start, heading, edge - start, 1),
            Target::Column(*metric),
        );
    }
    let found = app.found();
    let selected = app.selected();
    let at = selected
        .as_ref()
        .and_then(|selected| found.iter().position(|symbol| &symbol.id == selected))
        .unwrap_or(0);
    let room = usize::from(inside.height.saturating_sub(10)).max(1);
    let base = at.saturating_sub(room - 1);
    let (first, further) = scroll_window(app, Scroll::Results, base, found.len(), room);
    let list = Rect {
        y: heading + 1,
        height: u16::try_from(room).unwrap_or(u16::MAX),
        ..inside
    };
    canvas.hit(list, Target::List(Scroll::Results, further));
    let marks = app.tree_marks();
    for (index, symbol) in found.iter().enumerate().skip(first).take(room) {
        let y = heading + 1 + u16::try_from(index - first).unwrap_or(u16::MAX);
        let line = Rect {
            y,
            height: 1,
            ..inside
        };
        if let Some(mark) = marks.get(&symbol.id).filter(|mark| mark.starts_with('!')) {
            let token = if mark.starts_with("!!") {
                Token::H3
            } else {
                Token::H2
            };
            canvas.text(inside.x + 1, y, mark, token, line);
        }
        let name_room = Rect {
            width: kind_x.saturating_sub(inside.x + 2),
            ..line
        };
        canvas.text(inside.x + 4, y, symbol.id.as_str(), Token::Fg, name_room);
        canvas.text(kind_x, y, symbol.kind.name(), Token::Dim, line);
        for (metric, edge) in columns.iter().zip(&edges) {
            let value = app.shown_value(&symbol.id, *metric);
            let value = if value == "n/a" { String::new() } else { value };
            canvas.text_right(*edge, y, &value, Token::Fg, line);
        }
        canvas.hit(line, Target::Symbol(symbol.id.clone()));
        if selected.as_ref() == Some(&symbol.id) {
            canvas.paint(line, Token::Sel);
        }
    }
    let below = found.len().saturating_sub(first + room);
    if below > 0 {
        let y = heading + 1 + u16::try_from(room).unwrap_or(u16::MAX) + 1;
        canvas.text(
            inside.x + 1,
            y,
            &format!("{} more below", grouped(below)),
            Token::Faint,
            inside,
        );
    }
}

/// What `Tab` would complete the filter with, under the word being written.
fn completion_popup(app: &App, canvas: &mut Canvas<'_>, bar: Rect, typed: &str) {
    let completions = app.completions();
    if completions.is_empty() {
        return;
    }
    let word = typed.rsplit(' ').next().unwrap_or("");
    let start = bar.x + 3 + width(typed).saturating_sub(width(word));
    let shown = completions.len().min(4);
    let area = Rect {
        x: start.saturating_sub(2),
        y: bar.y + 1,
        width: 58,
        height: u16::try_from(shown).unwrap_or(4) + 3,
    }
    .intersection(canvas.area());
    canvas.fill(area, Token::Fg);
    canvas.frame(area, Token::Line, ['╭', '╮', '╰', '╯', '─', '│']);
    for (row, (name, description)) in completions.iter().take(shown).enumerate() {
        let y = area.y + 1 + u16::try_from(row).unwrap_or(0);
        canvas.hit(
            Rect::new(area.x + 1, y, area.width.saturating_sub(2), 1),
            Target::Entry(name.clone()),
        );
        canvas.text(area.x + 2, y, name, Token::Fg, area);
        canvas.text(area.x + 17, y, description, Token::Dim, area);
    }
    let y = area.y + 1 + u16::try_from(shown).unwrap_or(0);
    let end = canvas.text(area.x + 2, y, "Tab", Token::Acc, area);
    canvas.text(end, y, " complete", Token::Dim, area);
}

fn alerts(app: &App, canvas: &mut Canvas<'_>, body: Rect) {
    let left = Rect {
        width: 50.min(body.width / 2),
        ..body
    };
    let right = Rect {
        x: body.x + left.width,
        width: body.width - left.width,
        ..body
    };
    // The rules and the one selected take 18 of the design's 32 rows, and CI the rest.
    let rules_area = Rect {
        height: (body.height * 9 / 16).max(8).min(body.height),
        ..left
    };
    let ci_area = Rect {
        y: left.y + rules_area.height,
        height: left.height - rules_area.height,
        ..left
    };
    let active = app.pane();
    let rules = canvas.pane(rules_area, "Rules", active == Some(Pane::Rules));
    let Some(analysis) = app.analysis() else {
        return;
    };
    alerts_rules(app, analysis, canvas, rules);
    let rows = app.alert_rows();
    let title = if app.by_location() {
        format!("{} places", rows.len())
    } else {
        format!("{} hits, worst first", rows.len())
    };
    let hits = canvas.pane(right, &title, active == Some(Pane::Hits));
    if app.by_location() {
        for (row, place) in rows.iter().enumerate() {
            let y = hits.y + u16::try_from(row).unwrap_or(u16::MAX);
            let line = Rect {
                y,
                height: 1,
                ..hits
            };
            canvas.text(hits.x + 1, y, place, Token::Fg, line);
            if row == app.selected_hit() {
                canvas.paint(line, Token::Sel);
            }
        }
    } else {
        alerts_hits(app, analysis, canvas, hits);
    }
    alerts_ci(app, analysis, canvas, ci_area);
}

/// The hits of the selected rule, worst first, with where each is, and how the rule's
/// metric is spread around its line.
fn alerts_hits(app: &App, analysis: &ProjectAnalysis, canvas: &mut Canvas<'_>, hits: Rect) {
    let Some((_, rule)) = app.listed_rules().get(app.selected_rule()).copied() else {
        return;
    };
    let value_end = hits.x + 32;
    canvas.text(hits.x + 1, hits.y, "symbol", Token::Faint, hits);
    canvas.text_right(
        value_end,
        hits.y,
        short_metric(rule.metric),
        Token::Faint,
        hits,
    );
    canvas.text(hits.x + 33, hits.y, "where", Token::Faint, hits);
    let all = app.alert_hits();
    // The spread and the rows around it take 18 rows; the hits have the rest.
    let room = usize::from(hits.height.saturating_sub(18)).max(1);
    let base = app.selected_hit().saturating_sub(room - 1);
    let (first, further) = scroll_window(app, Scroll::Hits, base, all.len(), room);
    let list = Rect {
        y: hits.y + 1,
        height: u16::try_from(room).unwrap_or(u16::MAX),
        ..hits
    };
    canvas.hit(list, Target::List(Scroll::Hits, further));
    for (row, alert) in all.iter().enumerate().skip(first).take(room) {
        let y = hits.y + 1 + u16::try_from(row - first).unwrap_or(u16::MAX);
        let line = Rect {
            y,
            height: 1,
            ..hits
        };
        let name = Rect { width: 30, ..line };
        canvas.text(
            hits.x + 1,
            y,
            &short_subject(&alert.subject),
            Token::Fg,
            name,
        );
        let heat = rule.metric.descriptor().heat_of(rule.scope, alert.value);
        let value = crate::state::shown(rule.metric, alert.value);
        canvas.text_right(value_end, y, &value, heat_token(heat), line);
        let place = match &alert.subject {
            Subject::Symbol(id) => app
                .symbol(id)
                .and_then(|symbol| symbol.location.as_ref())
                .map(|location| format!("{}:{}", location.file, location.span.start_line)),
            Subject::File(path) => Some(path.to_string()),
        };
        if let Some(place) = place {
            canvas.text(hits.x + 33, y, &place, Token::Dim, line);
        }
        if row == app.selected_hit() {
            canvas.paint(line, Token::Sel);
        }
        if let Subject::Symbol(id) = &alert.subject {
            canvas.hit(line, Target::Symbol(id.clone()));
        }
    }
    let shown = room.min(all.len());
    let below = all.len().saturating_sub(first + shown);
    let mut y = hits.y + 1 + u16::try_from(room).unwrap_or(u16::MAX) + 1;
    if below > 0 {
        let more = format!("{} more below", grouped(below));
        canvas.text(hits.x + 1, y, &more, Token::Faint, hits);
    }
    y += 2;
    if let Some(spread) = analysis.rule_spread(rule, usize::from(hits.width.saturating_sub(4))) {
        rule_spread(rule, &spread, canvas, Rect { y, ..hits });
    }
}

/// A rule's metric across its scope, with the rule's line through it.
fn rule_spread(rule: &AlertRule, spread: &RuleSpread, canvas: &mut Canvas<'_>, area: Rect) {
    let x = area.x + 1;
    let y = area.y;
    let rule_line = "─".repeat(usize::from(area.width.saturating_sub(2)));
    canvas.text(x, y, &rule_line, Token::Faint, area);
    let scope = peers_name(rule.scope);
    let title = format!("Spread of {} across all {scope}", metric_label(rule.metric));
    canvas.text(x, y + 1, &title, Token::Faint, area);
    let mark = if rule.severity == Severity::Error {
        Token::H3
    } else {
        Token::H2
    };
    let at = x + u16::try_from(spread.line).unwrap_or(u16::MAX);
    canvas.text(at, y + 2, &format!("│{}", rule.when), mark, area);
    let glyphs = sparkline(&spread.counts);
    canvas.text(x, y + 3, &glyphs, Token::Dim, area);
    let worst: String = glyphs
        .chars()
        .nth(spread.worst)
        .map(String::from)
        .unwrap_or_default();
    let worst_at = x + u16::try_from(spread.worst).unwrap_or(u16::MAX);
    canvas.text(worst_at, y + 3, &worst, Token::H3, area);
    canvas.text(at, y + 4, "│", mark, area);
    let singular = scope.strip_suffix('s').unwrap_or(scope);
    let under = if spread.under == 1 {
        format!("1 {singular} is under the line.")
    } else {
        format!("{} {scope} are under the line.", grouped(spread.under))
    };
    let over = if spread.over == 1 {
        "1 is over it.".to_owned()
    } else {
        format!("{} are over it.", grouped(spread.over))
    };
    canvas.text(x, y + 5, &format!("{under} {over}"), Token::Dim, area);
    let end = canvas.text(x, y + 7, "e", Token::Acc, area);
    canvas.text(
        end,
        y + 7,
        " edit the rule: the line and hit count move as you type",
        Token::Dim,
        area,
    );
}

/// What a column of values is headed with: `cc` for cyclomatic, as the design has it,
/// and the metric's name otherwise.
const fn short_metric(metric: MetricId) -> &'static str {
    match metric {
        MetricId::Cyclomatic => "cc",
        _ => metric_label(metric),
    }
}

fn status(app: &App, canvas: &mut Canvas<'_>, body: Rect) {
    let left = Rect {
        width: 56.min(body.width / 2),
        ..body
    };
    let right = Rect {
        x: body.x + left.width,
        width: body.width - left.width,
        ..body
    };
    let split = |area: Rect, heights: [u16; 2]| -> [Rect; 3] {
        let first = heights[0].min(area.height);
        let second = heights[1].min(area.height - first);
        [
            Rect {
                height: first,
                ..area
            },
            Rect {
                y: area.y + first,
                height: second,
                ..area
            },
            Rect {
                y: area.y + first + second,
                height: area.height - first - second,
                ..area
            },
        ]
    };
    let [pipeline, problems, limits] = split(left, [13, 10]);
    let [precise, tests, index] = split(right, [13, 8]);
    status_pipeline(app, canvas, pipeline);
    status_problems(app, canvas, problems);
    status_limits(app, canvas, limits);
    status_precise(app, canvas, precise);
    status_tests(app, canvas, tests);
    status_index(app, canvas, index);
}

fn popups(app: &App, canvas: &mut Canvas<'_>, body: Rect) {
    if let Some(prefix) = app.hinting() {
        let entries = hints(prefix);
        let height = u16::try_from(entries.len()).unwrap_or_default() + 2;
        let area = Rect {
            x: body.x + 2,
            y: (body.y + body.height).saturating_sub(height + 1),
            width: 30.min(body.width),
            height,
        };
        popup_frame(canvas, area, &format!("{prefix} …"));
        for (index, (key, meaning)) in entries.iter().enumerate() {
            let y = area.y + 1 + u16::try_from(index).unwrap_or_default();
            canvas.text(area.x + 2, y, &key.to_string(), Token::Popk, area);
            canvas.text(area.x + 5, y, meaning, Token::Pop, area);
        }
    }
    match app.popup() {
        Some(Popup::SpaceMenu) => space_menu_popup(app, canvas, body),
        Some(Popup::MetricPicker { typed }) => metric_picker(canvas, body, typed),
        Some(Popup::Help { all }) => help(app, canvas, body, *all),
        Some(Popup::ConfirmPrecise) => confirm_precise(app, canvas, body),
        Some(Popup::SortPicker { typed }) => picker(canvas, body, "Sort by", typed),
        Some(Popup::RelationPicker {
            typed,
            ticked,
            cursor,
        }) => relation_picker(canvas, body, typed, ticked, *cursor),
        Some(Popup::MoreNodes { hidden, cursor }) => more_nodes(app, canvas, body, hidden, *cursor),
        Some(Popup::SmallTiles {
            tiles,
            cursor,
            packages,
        }) => small_tiles(app, canvas, body, tiles, *cursor, *packages),
        Some(Popup::ContextMenu { cursor }) => context_menu(app, canvas, body, *cursor),
        None => {}
    }
}

fn popup_frame(canvas: &mut Canvas<'_>, area: Rect, title: &str) {
    canvas.hit(area, Target::Popup);
    canvas.fill(area, Token::Pop);
    canvas.frame(area, Token::Popk, ['╭', '╮', '╰', '╯', '─', '│']);
    if !title.is_empty() {
        canvas.text(area.x + 2, area.y, &format!(" {title} "), Token::Popb, area);
    }
}

fn space_menu_popup(app: &App, canvas: &mut Canvas<'_>, body: Rect) {
    let groups = space_menu();
    let tallest = groups
        .iter()
        .map(|(_, entries)| entries.len())
        .max()
        .unwrap_or_default();
    let height = u16::try_from(tallest).unwrap_or_default() + 3;
    let area = Rect {
        x: body.x + 1,
        y: (body.y + body.height).saturating_sub(height + 1),
        width: body.width.saturating_sub(2),
        height,
    };
    popup_frame(canvas, area, "");
    let tests = if app.lens().tests { "on" } else { "off" };
    let mut x = area.x + 3;
    for (group, entries) in groups {
        canvas.text(x, area.y + 1, group, Token::Popb, area);
        for (index, (key, label)) in entries.iter().enumerate() {
            let y = area.y + 2 + u16::try_from(index).unwrap_or_default();
            let label = if *key == 't' {
                format!("{label}: {tests}")
            } else {
                (*label).to_owned()
            };
            canvas.text(x, y, &key.to_string(), Token::Popk, area);
            let end = canvas.text(x + 3, y, &label, Token::Pop, area);
            canvas.hit(
                Rect {
                    x,
                    y,
                    width: end - x,
                    height: 1,
                },
                Target::Menu(*key),
            );
        }
        x += 30;
    }
}

fn confirm_precise(app: &App, canvas: &mut Canvas<'_>, body: Rect) {
    let area = Rect {
        x: body.x + body.width.saturating_sub(64) / 2,
        y: body.y + 8.min(body.height.saturating_sub(11)),
        width: 64.min(body.width),
        height: 11.min(body.height),
    };
    popup_frame(canvas, area, "");
    let x = area.x + 2;
    canvas.text(x, area.y + 1, "Run precise analysis?", Token::Popb, area);
    for (row, line) in [
        "Indexing runs compiler tooling over the whole project.",
        "It is slow, and for Rust it may run this project's",
        "build scripts and proc macros.",
    ]
    .into_iter()
    .enumerate()
    {
        let y = area.y + 3 + u16::try_from(row).unwrap_or(0);
        canvas.text(x, y, line, Token::Pop, area);
    }
    let browsing = "You can keep browsing. It shows how long it has run.";
    canvas.text(x, area.y + 7, browsing, Token::Pop, area);
    let ready: Vec<&str> = app
        .indexers()
        .iter()
        .filter(|report| report.ready.is_ok())
        .map(|report| report.indexer.as_str())
        .collect();
    let run = if ready.is_empty() {
        "run it".to_owned()
    } else {
        format!("run {} now", ready.join(" and "))
    };
    let y = area.y + 9;
    let end = canvas.text(x, y, "y", Token::Popk, area);
    let end = canvas.text(end + 1, y, &run, Token::Pop, area);
    canvas.hit(Rect::new(x, y, end - x, 1), Target::Entry(run));
    let start = end + 4;
    let end = canvas.text(start, y, "n", Token::Popk, area);
    let end = canvas.text(end + 1, y, "not now", Token::Pop, area);
    canvas.hit(
        Rect::new(start, y, end - start, 1),
        Target::Entry("not now".to_owned()),
    );
}

fn metric_picker(canvas: &mut Canvas<'_>, body: Rect, typed: &str) {
    picker(canvas, body, "Metric", typed);
}

/// A popup's title and what is typed into it, at its top.
fn popup_prompt(canvas: &mut Canvas<'_>, area: Rect, title: &str, typed: &str) {
    canvas.text(area.x + 2, area.y + 1, title, Token::Popb, area);
    let end = canvas.text(area.x + 2, area.y + 2, "> ", Token::Popk, area);
    let end = canvas.text(end, area.y + 2, typed, Token::Pop, area);
    canvas.text(end, area.y + 2, " ", Token::On, area);
}

/// A list of metrics that narrows as you type.
fn picker(canvas: &mut Canvas<'_>, body: Rect, title: &str, typed: &str) {
    let choices = picked(typed);
    let height = u16::try_from(choices.len()).unwrap_or_default() + 7;
    let area = Rect {
        x: body.x + (body.width.saturating_sub(64)) / 2,
        y: body.y + 3.min(body.height.saturating_sub(height)),
        width: 64.min(body.width),
        height: height.min(body.height),
    };
    popup_frame(canvas, area, "");
    popup_prompt(canvas, area, title, typed);
    for (index, metric) in choices.iter().enumerate() {
        let y = area.y + 4 + u16::try_from(index).unwrap_or_default();
        let token = if index == 0 { Token::Pops } else { Token::Pop };
        canvas.hit(
            Rect::new(area.x + 1, y, area.width.saturating_sub(2), 1),
            Target::Entry(metric_label(*metric).to_owned()),
        );
        canvas.text(area.x + 2, y, metric_label(*metric), token, area);
        canvas.text(area.x + 18, y, metric.measures(), Token::Popd, area);
        if metric.descriptor().heat.is_none() {
            canvas.text_right(area.x + area.width - 2, y, "neutral", Token::Popd, area);
        }
    }
    let hint = "Type to narrow. Neutral metrics have no heat.";
    canvas.text(
        area.x + 2,
        area.y + area.height.saturating_sub(2),
        hint,
        Token::Popd,
        area,
    );
}

fn help(app: &App, canvas: &mut Canvas<'_>, body: Rect, all: bool) {
    let title = if all {
        "All keys".to_owned()
    } else {
        format!("{} keys", app.view().name())
    };
    let mut entries: Vec<(&str, &str)> = view_hints(app.view()).to_vec();
    if all {
        entries = View::ALL
            .iter()
            .flat_map(|view| view_hints(*view).iter().copied())
            .collect();
        entries.dedup();
    }
    let height = (u16::try_from(entries.len()).unwrap_or_default() + 4).min(body.height);
    let area = Rect {
        x: body.x + body.width.saturating_sub(64) / 2,
        y: body.y + 1,
        width: 64.min(body.width),
        height,
    };
    popup_frame(canvas, area, "Help");
    canvas.text(area.x + 2, area.y + 1, &title, Token::Popb, area);
    let room = usize::from(area.height.saturating_sub(4));
    let (first, further) = scroll_window(app, Scroll::Popup, 0, entries.len(), room);
    canvas.hit(area, Target::List(Scroll::Popup, further));
    for (index, (key, meaning)) in entries.iter().skip(first).take(room).enumerate() {
        let y = area.y + 3 + u16::try_from(index).unwrap_or_default();
        canvas.text(area.x + 2, y, key, Token::Popk, area);
        canvas.text(area.x + 12, y, &fit(meaning, 40), Token::Pop, area);
    }
}

/// The step an analysis is on, how far it has got, and what comes next.
fn running_lines(app: &App, running: &Running, canvas: &mut Canvas<'_>, inside: Rect) {
    let elapsed = app.now().saturating_sub(running.since) / 1000;
    if running.step == Some(Step::Indexing) || running.indexer.is_some() {
        let label = running.percent.map_or_else(
            || "Precise running".to_owned(),
            |percent| format!("Precise {percent}%"),
        );
        canvas.text(inside.x + 1, inside.y, &label, Token::Accb, inside);
        canvas.text(
            inside.x + 1,
            inside.y + 1,
            &format!("for {elapsed}s"),
            Token::Fg,
            inside,
        );
        let indexer = running.indexer.as_deref().unwrap_or("the indexer");
        canvas.text(inside.x + 1, inside.y + 2, indexer, Token::Fg, inside);
    } else {
        let step = running.step.unwrap_or(Step::Discover);
        let (done, of) = running.files;
        let mut label = capitalized(step.name());
        if step == Step::Extract && of > 0 {
            let share = f64::from(u32::try_from(done).unwrap_or(u32::MAX))
                / f64::from(u32::try_from(of).unwrap_or(u32::MAX));
            label = format!("{label} {}%", steps(share, 100));
        }
        canvas.text(inside.x + 1, inside.y, &label, Token::Accb, inside);
        if of > 0 {
            let files = format!("{done} of {of} files");
            canvas.text(inside.x + 1, inside.y + 2, &files, Token::Fg, inside);
        }
        let next: Vec<&str> = Step::PIPELINE
            .iter()
            .filter(|later| {
                Step::PIPELINE.iter().position(|one| one == *later)
                    > Step::PIPELINE.iter().position(|one| *one == step)
            })
            .filter(|later| !matches!(later, Step::Check | Step::Enrich))
            .map(|later| later.name())
            .collect();
        if !next.is_empty() {
            canvas.text(inside.x + 1, inside.y + 4, "Then", Token::Dim, inside);
            canvas.text(
                inside.x + 1,
                inside.y + 5,
                &next.join(", "),
                Token::Dim,
                inside,
            );
        }
    }
    canvas.text(
        inside.x + 1,
        inside.y + 9,
        "Keep browsing.",
        Token::Dim,
        inside,
    );
}

fn capitalized(word: &str) -> String {
    let mut letters = word.chars();
    letters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(letters).collect()
    })
}

/// The one-eyed watcher, in its pose, at its place in the Analysis panel.
fn mascot(app: &App, canvas: &mut Canvas<'_>, inside: Rect) {
    let x = inside.x + 21;
    let y = inside.y + 1;
    if x + crate::mascot::WIDTH > inside.x + inside.width
        || y + crate::mascot::HEIGHT > inside.y + inside.height
    {
        return;
    }
    let pixels = crate::mascot::pixels(app.pose(), app.now());
    canvas.art(x, y, &crate::mascot::cells(&pixels));
}

/// Eight ranges as one cell each, from `▁` to `▇`.
fn sparkline(spread: &[usize]) -> String {
    let glyphs = ['▁', '▂', '▃', '▄', '▅', '▆', '▇'];
    let highest = spread.iter().copied().max().unwrap_or(1).max(1);
    spread
        .iter()
        .map(|count| {
            if *count == 0 {
                ' '
            } else {
                let level = (count * 7).div_ceil(highest).saturating_sub(1);
                glyphs.get(level.min(6)).copied().unwrap_or('▁')
            }
        })
        .collect()
}

/// What the metrics list knows before the analysis is done: the highest so far of what
/// one file is enough for, and what the rest is waiting for.
fn partial_metrics(app: &App, canvas: &mut Canvas<'_>, inside: Rect) {
    canvas.text(inside.x + 1, inside.y, "metric", Token::Faint, inside);
    canvas.text(
        inside.x + 25,
        inside.y,
        "highest so far",
        Token::Faint,
        inside,
    );
    canvas.text_right(
        inside.x + inside.width - 1,
        inside.y,
        "value",
        Token::Faint,
        inside,
    );
    let running = app.running_analysis();
    let partial = running.and_then(|running| running.partial.as_ref());
    for (index, metric) in OVERVIEW_METRICS.iter().enumerate() {
        let y = inside.y + 1 + u16::try_from(index).unwrap_or_default();
        if y >= inside.y + inside.height {
            break;
        }
        let line = Rect {
            y,
            height: 1,
            ..inside
        };
        canvas.text(inside.x + 1, y, metric_label(*metric), Token::Fg, line);
        let highest = partial.and_then(|partial| {
            partial
                .functions
                .iter()
                .filter(|function| app.lens().tests || function.role == CodeRole::Production)
                .filter_map(|function| {
                    let value = function
                        .values
                        .iter()
                        .find(|(measured, _)| measured == metric)?
                        .1;
                    Some((function, value))
                })
                .max_by(|(_, a), (_, b)| a.total_cmp(b))
        });
        if let Some((function, value)) = highest {
            canvas.text(inside.x + 25, y, &function.name, Token::Fg, line);
            let shown = crate::state::shown(*metric, value);
            canvas.text_right(inside.x + inside.width - 1, y, &shown, Token::Fg, line);
        } else {
            let waiting = match metric {
                MetricId::Churn | MetricId::Hotspot => "waiting for git history",
                MetricId::Duplication if partial.is_none() => "waiting for every file",
                MetricId::Duplication => "waiting for analyze",
                _ if running.is_some_and(|running| running.past(Step::Link)) => {
                    "waiting for analyze"
                }
                _ if partial.is_some() => "waiting for link",
                _ => "waiting for extract",
            };
            canvas.text(inside.x + 25, y, waiting, Token::Dim, line);
        }
        if *metric == app.lens().metric {
            canvas.paint(line, Token::Sel);
        }
    }
}

fn inspector_clones(
    analysis: &ProjectAnalysis,
    symbol: &Symbol,
    canvas: &mut Canvas<'_>,
    body: Rect,
) {
    let copies = symbol
        .location
        .as_ref()
        .and_then(|location| analysis.duplication().copies.get(&location.file));
    let Some(copies) = copies.filter(|copies| !copies.is_empty()) else {
        canvas.text(body.x + 1, body.y, "No clones.", Token::Dim, body);
        return;
    };
    canvas.text(body.x + 1, body.y, "Copies of its file", Token::B, body);
    let mut y = body.y + 1;
    for (file, ranges) in copies {
        for (start, end) in ranges {
            let line = format!("{file}  lines {start}-{end}");
            canvas.text(body.x + 1, y, &line, Token::Fg, body);
            y += 1;
        }
    }
}

/// The source of the focus with the points each line added, beside the reasons.
fn why_view(app: &App, canvas: &mut Canvas<'_>, body: Rect) {
    let Some((path, text)) = app.why_source() else {
        return;
    };
    let side = 40.min(body.width / 2);
    let left = Rect {
        width: body.width - side,
        ..body
    };
    let right = Rect {
        x: body.x + body.width - side,
        width: side,
        ..body
    };
    let title = format!("{path}   gutter: cognitive points");
    let inside = canvas.pane(left, &title, app.pane() != Some(Pane::Inspector));
    let points: HashMap<u32, u32> = app.gutter().into_iter().collect();
    let current = app.why_line();
    let first = app.why_top();
    for (offset, line) in text
        .lines()
        .skip(usize::try_from(first - 1).unwrap_or(0))
        .take(usize::from(inside.height))
        .enumerate()
    {
        let number = first + u32::try_from(offset).unwrap_or(0);
        let y = inside.y + u16::try_from(offset).unwrap_or(0);
        let row = Rect {
            y,
            height: 1,
            ..inside
        };
        canvas.text_right(inside.x + 5, y, &number.to_string(), Token::Faint, row);
        canvas.text(inside.x + 6, y, line, Token::Fg, row);
        // The points sit in the indentation, where the code has none.
        if let Some(added) = points.get(&number) {
            canvas.text(inside.x + 6, y, &format!("+{added}"), Token::H1, row);
        }
        if number == current {
            canvas.paint(row, Token::Sel);
        }
        canvas.hit(row, Target::Line(number));
    }
    why_reasons(app, canvas, right);
}

/// Beside the source: why the lens's metric is what it is, and why the file is a hotspot.
fn why_reasons(app: &App, canvas: &mut Canvas<'_>, area: Rect) {
    let (Some(analysis), Some(symbol)) = (
        app.analysis(),
        app.focus().and_then(|focus| app.symbol(focus)),
    ) else {
        return;
    };
    let active = app.pane() == Some(Pane::Inspector);
    let metric = app.lens().metric;
    let history = matches!(metric, MetricId::Hotspot | MetricId::Churn);
    let top = Rect {
        height: if history {
            area.height
        } else {
            14.min(area.height)
        },
        ..area
    };
    let value = app.shown_value(&symbol.id, metric);
    let title = format!("Why {} is {value}", metric_label(metric));
    let inside = canvas.pane(top, &title, active);
    if history {
        why_history(analysis, symbol, canvas, inside);
        return;
    }
    why_score(app, analysis, symbol, canvas, inside);
    let bottom = Rect {
        y: area.y + top.height,
        height: area.height - top.height,
        ..area
    };
    if bottom.height < 3 {
        return;
    }
    let file = symbol
        .location
        .as_ref()
        .map(|location| Subject::File(location.file.clone()));
    let hotspot = file
        .and_then(|file| analysis.value(&file, MetricId::Hotspot))
        .map_or_else(
            || "n/a".to_owned(),
            |value| crate::state::shown(MetricId::Hotspot, value),
        );
    let inside = canvas.pane(bottom, &format!("Why hotspot is {hotspot}"), false);
    why_history(analysis, symbol, canvas, inside);
}

/// A function's score, split into its decision points and what nesting added.
fn why_score(
    app: &App,
    analysis: &ProjectAnalysis,
    symbol: &Symbol,
    canvas: &mut Canvas<'_>,
    inside: Rect,
) {
    let metric = app.lens().metric;
    let x = inside.x + 1;
    let explained = matches!(metric, MetricId::Cyclomatic | MetricId::Cognitive)
        .then(|| analysis.explain(&symbol.id))
        .flatten();
    let Some(explanation) = explained else {
        for (row, line) in why_lines(app, analysis, symbol, metric).iter().enumerate() {
            let y = inside.y + 1 + u16::try_from(row).unwrap_or(u16::MAX);
            canvas.text(x, y, line, Token::Fg, inside);
        }
        return;
    };
    let most = explanation
        .kinds
        .iter()
        .map(|(_, count)| *count)
        .max()
        .unwrap_or(1)
        .max(1);
    let mut y = inside.y + 1;
    for (kind, count) in &explanation.kinds {
        canvas.text(x, y, kind, Token::Fg, inside);
        canvas.text_right(x + 20, y, &count.to_string(), Token::Fg, inside);
        let (filled, rest) = bar(f64::from(*count), f64::from(most), 13);
        let end = canvas.text(x + 21, y, &filled, Token::Acc, inside);
        canvas.text(end, y, &rest, Token::Track, inside);
        y += 1;
    }
    y += 1;
    canvas.text(x, y, "Decision points", Token::Fg, inside);
    canvas.text_right(
        x + 20,
        y,
        &explanation.decisions.to_string(),
        Token::Fg,
        inside,
    );
    if metric == MetricId::Cognitive {
        canvas.text(x, y + 1, "Added by nesting", Token::Fg, inside);
        let nesting = format!("+{}", explanation.nesting);
        canvas.text_right(x + 20, y + 1, &nesting, Token::Fg, inside);
        let total = explanation.decisions + explanation.nesting;
        canvas.text(
            x,
            y + 3,
            &nesting_share(explanation.nesting, total),
            Token::Dim,
            inside,
        );
    }
    if let Some((line, structures)) = &explanation.deepest {
        let deepest = format!("Deepest: {} levels, at line {line}.", structures.len());
        canvas.text(x, y + 4, &deepest, Token::Dim, inside);
    }
}

/// How much of a cognitive score nesting makes, in words.
fn nesting_share(nesting: u32, total: u32) -> String {
    if total == 0 || nesting == 0 {
        return "Nothing is added by nesting.".to_owned();
    }
    let share = f64::from(nesting) / f64::from(total);
    let words = [
        (0.25, "A quarter"),
        (1.0 / 3.0, "A third"),
        (0.5, "Half"),
        (2.0 / 3.0, "Two thirds"),
        (0.75, "Three quarters"),
    ];
    let nearest = words
        .iter()
        .min_by(|(a, _), (b, _)| (a - share).abs().total_cmp(&(b - share).abs()))
        .map_or("Some", |(_, words)| words);
    let said = if share < 0.15 {
        "Little"
    } else if share > 0.85 {
        "Nearly all"
    } else {
        nearest
    };
    format!("{said} of the score is nesting.")
}

/// Why a file is a hotspot: its churn and complexity, and the commits behind them.
fn why_history(analysis: &ProjectAnalysis, symbol: &Symbol, canvas: &mut Canvas<'_>, inside: Rect) {
    let x = inside.x + 1;
    if let Some(reason) = analysis.why_unavailable(MetricId::Churn) {
        canvas.text(
            x,
            inside.y + 1,
            &format!("History: {reason}"),
            Token::Dim,
            inside,
        );
        return;
    }
    let Some(location) = &symbol.location else {
        return;
    };
    let file = Subject::File(location.file.clone());
    let mut first = true;
    for (row, (label, metric, unit)) in [
        ("Churn", MetricId::Churn, "commits"),
        ("Complexity", MetricId::Cyclomatic, "in file"),
    ]
    .into_iter()
    .enumerate()
    {
        let y = inside.y + 1 + u16::try_from(row).unwrap_or(0);
        let value = analysis.value(&file, metric).map_or_else(
            || "n/a".to_owned(),
            |value| crate::state::shown(metric, value),
        );
        canvas.text(x, y, label, Token::Fg, inside);
        let end = x + 25;
        canvas.text_right(end, y, &value, Token::Fg, inside);
        canvas.text(end + 1, y, unit, Token::Dim, inside);
        first &= analysis
            .rank(&file, metric)
            .is_some_and(|rank| rank.position == 1);
    }
    let standing = if first {
        "Highest in the project on both.".to_owned()
    } else {
        let rank = |metric| {
            analysis
                .rank(&file, metric)
                .map_or_else(|| "n/a".to_owned(), |rank| format!("#{}", rank.position))
        };
        format!(
            "{} in churn, {} in complexity.",
            rank(MetricId::Churn),
            rank(MetricId::Cyclomatic)
        )
    };
    canvas.text(x, inside.y + 3, &standing, Token::Dim, inside);
    canvas.text(
        x,
        inside.y + 5,
        "Recent commits to this file",
        Token::Faint,
        inside,
    );
    let commits = analysis.commits_of(&location.file);
    for (row, commit) in commits.iter().take(7).enumerate() {
        let y = inside.y + 6 + u16::try_from(row).unwrap_or(0);
        let age = age(analysis.age_of(commit));
        let line = format!("{} {age:<3} {}", commit.short_id, commit.subject);
        canvas.text(x, y, &line, Token::Fg, inside);
    }
    let y = inside.y + 6 + u16::try_from(commits.len().min(7)).unwrap_or(0) + 1;
    canvas.text(x, y, "Last 12 months", Token::Faint, inside);
    let months = analysis.monthly_commits(&location.file);
    canvas.text(x, y + 1, &sparkline(&months), Token::Acc, inside);
}

/// How long ago, in the largest unit that fits: `3d`, `2w`, `5mo` or `2y`.
fn age(seconds: i64) -> String {
    const DAY: i64 = 24 * 60 * 60;
    let days = seconds.max(0) / DAY;
    match days {
        0..=13 => format!("{days}d"),
        14..=59 => format!("{}w", days / 7),
        60..=364 => format!("{}mo", days / 30),
        _ => format!("{}y", days / 365),
    }
}

/// The rule being written, what it would fire on, and the change to the file.
fn rule_editor(app: &App, editor: &RuleEditor, canvas: &mut Canvas<'_>, body: Rect) {
    let top = Rect {
        height: body.height.saturating_sub(12).max(10).min(body.height),
        ..body
    };
    let left = Rect {
        width: 54.min(top.width / 2),
        ..top
    };
    let right = Rect {
        x: top.x + left.width,
        width: top.width - left.width,
        ..top
    };
    let title = if editor.existing.is_some() {
        "Rule"
    } else {
        "New rule"
    };
    let inside = canvas.pane(left, title, true);
    for (index, (name, value)) in FIELDS.iter().zip(&editor.values).enumerate() {
        let y = inside.y + 1 + u16::try_from(index).unwrap_or(0);
        let row = Rect {
            y,
            height: 1,
            ..inside
        };
        canvas.text(inside.x + 1, y, name, Token::Dim, row);
        let shown = match (&editor.typing, index == editor.field) {
            (Some(typed), true) => format!("{typed}█"),
            _ => value.clone(),
        };
        canvas.text(inside.x + 12, y, &shown, Token::Fg, row);
        if index == editor.field {
            canvas.paint(row, Token::Sel);
        }
    }
    let notes = inside.y + 9;
    canvas.text(
        inside.x + 1,
        notes,
        "i edit field   Tab next field",
        Token::Dim,
        inside,
    );
    canvas.text(
        inside.x + 1,
        notes + 1,
        ":w save rule  :w user keep it yours  :q discard",
        Token::Dim,
        inside,
    );
    if let Some(error) = &editor.error {
        canvas.text(inside.x + 1, notes + 3, error, Token::H2, inside);
    }
    let hits = app.editor_hits();
    let preview = canvas.pane(
        right,
        &format!("Preview: would fire on {hits} symbols"),
        false,
    );
    if let (Some(analysis), Ok(rule)) = (app.analysis(), editor.rule()) {
        let mut y = preview.y + 1;
        for alert in analysis.fire(std::slice::from_ref(&rule)).iter().take(8) {
            let value = crate::state::shown(alert.metric, alert.value);
            canvas.text(
                preview.x + 1,
                y,
                &format!("!  {}", alert.subject),
                Token::Fg,
                preview,
            );
            canvas.text_right(preview.x + preview.width - 2, y, &value, Token::Fg, preview);
            y += 1;
        }
        if let Some((subject, value)) = analysis.next_closest(&rule) {
            let value = crate::state::shown(rule.metric, value);
            let line = format!("Next closest: {subject} at {value}.");
            canvas.text(preview.x + 1, y + 1, &line, Token::Dim, preview);
        }
    }
    let bottom = Rect {
        y: top.y + top.height,
        height: body.height - top.height,
        ..body
    };
    rule_writes(editor, canvas, bottom);
}

fn status_pipeline(app: &App, canvas: &mut Canvas<'_>, area: Rect) {
    let title = app.analysis().map_or_else(
        || "Last refresh".to_owned(),
        |analysis| {
            let millis: u64 = analysis.steps().iter().map(|step| step.millis).sum();
            let changed = analysis.files_parsed();
            let files = if changed == 1 { "file" } else { "files" };
            format!(
                "Last refresh: {changed} {files} changed, {}.{}s",
                millis / 1000,
                (millis % 1000) / 100
            )
        },
    );
    let inside = canvas.pane(area, &title, app.pane() == Some(Pane::Status));
    if let Some(failure) = app.failure() {
        canvas.text(inside.x + 1, inside.y, failure, Token::H2, inside);
    }
    let Some(analysis) = app.analysis() else {
        return;
    };
    canvas.text(inside.x + 1, inside.y, "step", Token::Faint, inside);
    canvas.text(inside.x + 14, inside.y, "time", Token::Faint, inside);
    canvas.text(inside.x + 19, inside.y, "result", Token::Faint, inside);
    for (row, report) in analysis.steps().iter().enumerate() {
        let y = inside.y + 1 + u16::try_from(row).unwrap_or(u16::MAX);
        let skipped = report.result.starts_with("skipped");
        let seconds = if skipped {
            "-".to_owned()
        } else {
            format!("{}.{}s", report.millis / 1000, (report.millis % 1000) / 100)
        };
        canvas.text(
            inside.x + 1,
            y,
            &capitalized(report.step.name()),
            Token::Fg,
            inside,
        );
        canvas.text_right(inside.x + 18, y, &seconds, Token::Fg, inside);
        let token = if skipped { Token::Dim } else { Token::Fg };
        canvas.text(inside.x + 19, y, &report.result, token, inside);
    }
}

fn status_problems(app: &App, canvas: &mut Canvas<'_>, area: Rect) {
    let problems: Vec<String> = app
        .analysis()
        .map(|analysis| {
            analysis
                .problems()
                .iter()
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default();
    let title = if problems.is_empty() {
        "Problems: none".to_owned()
    } else {
        format!("Problems: {}", plural(problems.len(), "file"))
    };
    let inside = canvas.pane(area, &title, false);
    if problems.is_empty() {
        canvas.text(
            inside.x + 1,
            inside.y,
            "Every file was read.",
            Token::Dim,
            inside,
        );
        return;
    }
    for (row, problem) in problems.iter().enumerate() {
        let y = inside.y + u16::try_from(row).unwrap_or(u16::MAX);
        let line = Rect {
            y,
            height: 1,
            ..inside
        };
        if app.problem_at() == Some(row) {
            canvas.fill(line, Token::Sel);
        }
        canvas.text(inside.x + 1, y, problem, Token::Fg, line);
        canvas.hit(line, Target::Problem(row));
    }
    let y = inside.y + u16::try_from(problems.len()).unwrap_or(u16::MAX) + 1;
    for (offset, text) in [
        "Everything else was analyzed as usual. Code around",
        "a syntax error still counts, as far as it can be",
        "understood.",
    ]
    .iter()
    .enumerate()
    {
        let row = y + u16::try_from(offset).unwrap_or(0);
        canvas.text(inside.x + 1, row, text, Token::Dim, inside);
    }
}

fn status_precise(app: &App, canvas: &mut Canvas<'_>, area: Rect) {
    let inside = canvas.pane(area, "Precise analysis", false);
    canvas.text(inside.x + 1, inside.y, "language", Token::Faint, inside);
    canvas.text(inside.x + 26, inside.y, "indexer", Token::Faint, inside);
    canvas.text(inside.x + 43, inside.y, "ready?", Token::Faint, inside);
    let mut y = inside.y + 1;
    for report in app.indexers() {
        let languages: Vec<&str> = report
            .languages
            .iter()
            .map(|language| match language {
                cyclops_domain::language::LanguageId::Rust => "Rust",
                cyclops_domain::language::LanguageId::TypeScript => "TypeScript",
                cyclops_domain::language::LanguageId::JavaScript => "JavaScript",
            })
            .collect();
        canvas.text(inside.x + 1, y, &languages.join(", "), Token::Fg, inside);
        canvas.text(inside.x + 26, y, &report.indexer, Token::Fg, inside);
        match &report.ready {
            Ok(()) => {
                canvas.text(inside.x + 43, y, "yes", Token::Fg, inside);
            }
            Err(reason) => {
                canvas.text(inside.x + 43, y, "no", Token::H1, inside);
                y += 1;
                canvas.text(inside.x + 1, y, reason, Token::Dim, inside);
            }
        }
        y += 1;
    }
    let Some(analysis) = app.analysis() else {
        return;
    };
    y += 1;
    if let Some(running) = app.running_analysis().filter(|running| running.precise) {
        let indexer = running.indexer.as_deref().unwrap_or("the indexer");
        let since = elapsed(app.now().saturating_sub(running.since));
        let progress = running.percent.map_or_else(
            || "No progress reported.".to_owned(),
            |percent| format!("{percent}% done."),
        );
        let line = format!("Running {indexer} for {since}. {progress}");
        canvas.text(inside.x + 1, y, &line, Token::Fg, inside);
        y += 2;
    }
    canvas.text(inside.x + 1, y, "Relations now", Token::Faint, inside);
    let relations: Vec<_> = analysis.view().relations().collect();
    let total = relations.len().max(1);
    for (offset, confidence) in [
        Confidence::Exact,
        Confidence::Inferred,
        Confidence::Ambiguous,
    ]
    .into_iter()
    .enumerate()
    {
        let row = y + 1 + u16::try_from(offset).unwrap_or(0);
        let count = relations
            .iter()
            .filter(|relation| relation.confidence == confidence)
            .count();
        let share = f64::from(u32::try_from(count).unwrap_or(u32::MAX))
            / f64::from(u32::try_from(total).unwrap_or(u32::MAX));
        canvas.text(inside.x + 1, row, confidence.name(), Token::Fg, inside);
        canvas.text_right(
            inside.x + 20,
            row,
            &format!("{}%", steps(share, 100)),
            Token::Fg,
            inside,
        );
        let (filled, rest) = bar(share, 1.0, 38);
        let token = if confidence == Confidence::Ambiguous {
            Token::H1
        } else {
            Token::Acc
        };
        let x = canvas.text(inside.x + 21, row, &filled, token, inside);
        canvas.text(x, row, &rest, Token::Track, inside);
    }
    let row = y + 5;
    let x = canvas.text(inside.x + 1, row, "semantic.mode ", Token::Dim, inside);
    let x = canvas.text(x, row, analysis.semantic_mode().name(), Token::Fg, inside);
    let x = canvas.text(x, row, "   index ", Token::Dim, inside);
    let index = match analysis.precise().current {
        None => "none yet".to_owned(),
        Some(_) => format!("{} changed", plural(analysis.precise().changed, "file")),
    };
    canvas.text(x, row, &index, Token::Fg, inside);
}

/// What a value is made of, line by line, for the metric of the lens.
fn why_lines(
    app: &App,
    analysis: &ProjectAnalysis,
    symbol: &Symbol,
    metric: MetricId,
) -> Vec<String> {
    let name = |id: &cyclops_domain::symbol::SymbolId| id.to_string();
    match metric {
        MetricId::Lcom => analysis
            .method_groups(&symbol.id)
            .unwrap_or_default()
            .iter()
            .enumerate()
            .flat_map(|(index, group)| {
                [
                    format!("{}  {}", index + 1, group.methods.join(", ")),
                    format!("   fields {}", group.fields.join(", ")),
                    String::new(),
                ]
            })
            .collect(),
        MetricId::Members => {
            let children = analysis.children(&symbol.id);
            let fields: Vec<&str> = children
                .iter()
                .filter(|child| child.kind == SymbolKind::Field)
                .map(|child| child.name.as_str())
                .collect();
            let methods: Vec<&str> = children
                .iter()
                .filter(|child| child.kind == SymbolKind::Method)
                .map(|child| child.name.as_str())
                .collect();
            vec![
                format!("Fields  {}", fields.join(", ")),
                format!("Methods {}", methods.join(", ")),
            ]
        }
        MetricId::Afferent
        | MetricId::Efferent
        | MetricId::Instability
        | MetricId::Abstractness
        | MetricId::Distance => coupling_lines(app, analysis, symbol),
        MetricId::CycleSize => match analysis.cycle_path(&symbol.id) {
            Some(path) => {
                let mut hops: Vec<String> = path.iter().map(|(from, _, _)| name(from)).collect();
                if let Some(first) = hops.first().cloned() {
                    hops.push(first);
                }
                vec![hops.join(" -> ")]
            }
            None => vec!["Not in a cycle.".to_owned()],
        },
        MetricId::Duplication => {
            let copies = symbol
                .location
                .as_ref()
                .and_then(|location| analysis.duplication().copies.get(&location.file));
            match copies {
                Some(copies) if !copies.is_empty() => copies
                    .iter()
                    .flat_map(|(file, ranges)| {
                        ranges
                            .iter()
                            .map(move |(start, end)| format!("{file}  lines {start}-{end}"))
                    })
                    .collect(),
                _ => vec!["No copies.".to_owned()],
            }
        }
        MetricId::CommentLines | MetricId::BlankLines => vec!["The value for the file.".to_owned()],
        _ => source_reasons(analysis, symbol),
    }
}

/// The modules on each side of a module, and how its instability follows from them.
fn coupling_lines(app: &App, analysis: &ProjectAnalysis, symbol: &Symbol) -> Vec<String> {
    let name = |id: &cyclops_domain::symbol::SymbolId| id.to_string();
    let relations = app.relations_of(&symbol.id);
    let modules = |incoming: bool| -> Vec<String> {
        let mut found: Vec<String> = relations
            .iter()
            .filter(|(other, kind, _, used)| {
                *used == incoming
                    && *kind == cyclops_domain::relation::RelationKind::Imports
                    && app
                        .symbol(other)
                        .is_some_and(|symbol| symbol.kind == SymbolKind::Module)
            })
            .map(|(other, ..)| name(other))
            .collect();
        found.dedup();
        found
    };
    let out = modules(false);
    let into = modules(true);
    let mut lines = vec![format!("Depends on {}", out.len())];
    lines.extend(out.iter().cloned());
    lines.push(String::new());
    lines.push(format!("Depended on by {}", into.len()));
    lines.extend(into.iter().cloned());
    // How instability follows from the two, with the value the core measured.
    if let Some(instability) =
        analysis.value(&Subject::Symbol(symbol.id.clone()), MetricId::Instability)
    {
        lines.push(String::new());
        lines.push(format!(
            "Instability {} / ({} + {}) = {}",
            out.len(),
            out.len(),
            into.len(),
            crate::state::shown(MetricId::Instability, instability)
        ));
    }
    lines
}

/// The parts of a function's score, and the history of its file.
fn source_reasons(analysis: &ProjectAnalysis, symbol: &Symbol) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(explanation) = analysis.explain(&symbol.id) {
        for (kind, count) in &explanation.kinds {
            lines.push(format!("{kind:<16} {count}"));
        }
        lines.push(format!("Decision points  {}", explanation.decisions));
        lines.push(format!("Added by nesting +{}", explanation.nesting));
        if let Some((line, _)) = &explanation.deepest {
            lines.push(format!("Deepest at line {line}."));
        }
    }
    lines.push(String::new());
    if let Some(reason) = analysis.why_unavailable(MetricId::Churn) {
        lines.push(format!("History: {reason}"));
    } else if let Some(location) = &symbol.location {
        lines.push("Recent commits to this file".to_owned());
        for commit in analysis.commits_of(&location.file).iter().take(8) {
            lines.push(format!("{} {}", commit.short_id, commit.subject));
        }
    }
    lines
}

/// What `cyclops check` would do now.
fn alerts_ci(app: &App, analysis: &ProjectAnalysis, canvas: &mut Canvas<'_>, ci_area: Rect) {
    let ci = canvas.pane(ci_area, "If CI ran now", false);
    if let Some((errors, warnings, code, yours)) = app.ci_verdict() {
        let fail_on = if analysis.fail_on() == Severity::Error {
            "error"
        } else {
            "warning"
        };
        canvas.text(ci.x + 1, ci.y, "cyclops check", Token::B, ci);
        canvas.text(
            ci.x + 1,
            ci.y + 2,
            &format!("fail_on      {fail_on}"),
            Token::Fg,
            ci,
        );
        canvas.text(
            ci.x + 1,
            ci.y + 3,
            &format!("errors       {errors}"),
            Token::Fg,
            ci,
        );
        canvas.text(
            ci.x + 1,
            ci.y + 4,
            &format!("warnings     {warnings}"),
            Token::Fg,
            ci,
        );
        let verdict = if code == 0 {
            "Exit code 0: the build would pass.".to_owned()
        } else {
            format!("Exit code {code}: this would fail the build.")
        };
        canvas.text(ci.x + 1, ci.y + 5, &verdict, Token::Fg, ci);
        if yours > 0 {
            let rules = if yours == 1 { "rule" } else { "rules" };
            let line = format!("Leaves out {yours} {rules} of yours.");
            canvas.text(ci.x + 1, ci.y + 6, &line, Token::Dim, ci);
        }
        ci_errors(
            app,
            analysis,
            canvas,
            Rect {
                y: ci.y + 8,
                height: ci.height.saturating_sub(8),
                ..ci
            },
        );
    }
}

/// The errors that would fail the build, and the rule they come from if only one.
fn ci_errors(app: &App, analysis: &ProjectAnalysis, canvas: &mut Canvas<'_>, area: Rect) {
    let errors = app.counted_errors();
    let Some(first) = errors.first() else {
        return;
    };
    let rule_of = |alert: &cyclops_application::queries::Alert| {
        analysis.rules().get(alert.rule_index).map(AlertRule::title)
    };
    let one_rule = errors
        .iter()
        .all(|alert| alert.rule_index == first.rule_index);
    let said = match (one_rule.then(|| rule_of(first)).flatten(), errors.len()) {
        (Some(rule), 1) => format!("The error is in {rule}:"),
        (Some(rule), 2) => format!("Both errors are in {rule}:"),
        (Some(rule), count) => format!("All {count} errors are in {rule}:"),
        (None, _) => "The errors:".to_owned(),
    };
    canvas.text(area.x + 1, area.y, &said, Token::Fg, area);
    for (row, alert) in errors.iter().enumerate() {
        let y = area.y + 1 + u16::try_from(row).unwrap_or(u16::MAX);
        if y >= area.y + area.height {
            break;
        }
        let name = Rect { width: 26, ..area };
        canvas.text(area.x + 1, y, "!!", Token::H3, area);
        canvas.text(
            area.x + 4,
            y,
            &short_subject(&alert.subject),
            Token::Fg,
            name,
        );
        canvas.text(area.x + 27, y, metric_label(alert.metric), Token::Dim, area);
        let value = crate::state::shown(alert.metric, alert.value);
        canvas.text(area.x + 32, y, &value, Token::Fg, area);
    }
}

/// The rules, errors first, with their hits.
fn alerts_rules(app: &App, analysis: &ProjectAnalysis, canvas: &mut Canvas<'_>, rules: Rect) {
    let listed = app.listed_rules();
    if listed.is_empty() {
        canvas.text(
            rules.x + 1,
            rules.y,
            "No alert rules yet.",
            Token::Dim,
            rules,
        );
        canvas.text(
            rules.x + 1,
            rules.y + 1,
            "A starts one from the focus.",
            Token::Dim,
            rules,
        );
    } else {
        canvas.text(rules.x + 4, rules.y, "hits name", Token::Faint, rules);
    }
    for (row, (index, rule)) in listed.iter().enumerate() {
        let y = rules.y + 1 + u16::try_from(row).unwrap_or(u16::MAX);
        let line = Rect {
            y,
            height: 1,
            ..rules
        };
        let hits = analysis
            .alerts()
            .iter()
            .filter(|alert| alert.rule_index == *index)
            .count();
        let (mark, token) = match (rule.enabled, rule.severity) {
            (false, _) => ("-", Token::Dim),
            (true, Severity::Error) => ("!!", Token::H3),
            (true, Severity::Warning) => ("!", Token::H2),
        };
        let mut title = rule.title();
        if rule.origin == cyclops_domain::settings::Origin::UserFile {
            title.push_str(" (yours)");
        }
        if !rule.enabled {
            title.push_str(" (disabled)");
        }
        canvas.text(rules.x + 1, y, mark, token, line);
        canvas.text_right(rules.x + 8, y, &hits.to_string(), Token::Fg, line);
        canvas.text(rules.x + 9, y, &title, Token::Fg, line);
        if row == app.selected_rule() {
            canvas.paint(line, Token::Sel);
        }
        canvas.hit(line, Target::Rule(row));
    }
    if let Some((_, rule)) = listed.get(app.selected_rule()) {
        let y = rules.y + 1 + u16::try_from(listed.len()).unwrap_or(u16::MAX);
        rule_details(rule, canvas, Rect { y, ..rules });
    }
}

/// The rule selected, field by field, and the file it is defined in.
fn rule_details(rule: &AlertRule, canvas: &mut Canvas<'_>, area: Rect) {
    let x = area.x + 1;
    let divider = "─".repeat(usize::from(area.width.saturating_sub(2)));
    canvas.text(x, area.y, &divider, Token::Faint, area);
    canvas.text(x, area.y + 1, &rule.title(), Token::B, area);
    let paths = if rule.paths.is_empty() {
        "all".to_owned()
    } else {
        rule.paths
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    };
    let severity = if rule.severity == Severity::Error {
        "error"
    } else {
        "warning"
    };
    let fields = [
        ("metric", metric_label(rule.metric).to_owned()),
        ("scope", rule.scope.name().to_owned()),
        ("when", rule.when.clone()),
        ("severity", severity.to_owned()),
        ("paths", paths),
    ];
    for (row, (label, value)) in fields.iter().enumerate() {
        let y = area.y + 2 + u16::try_from(row).unwrap_or(u16::MAX);
        canvas.text(x, y, label, Token::Dim, area);
        canvas.text(x + 11, y, value, Token::Fg, area);
    }
    let defined = match rule.origin {
        cyclops_domain::settings::Origin::UserFile => "Defined in ~/.config/cyclops/config.toml",
        cyclops_domain::settings::Origin::ProjectFile => "Defined in .cyclops/config.toml",
        cyclops_domain::settings::Origin::Environment => "Defined in the environment",
        cyclops_domain::settings::Origin::Flag => "Defined by a flag",
    };
    canvas.text(x, area.y + 8, defined, Token::Faint, area);
}

/// The change the rule makes to its settings file.
fn rule_writes(editor: &RuleEditor, canvas: &mut Canvas<'_>, bottom: Rect) {
    let file = if editor.existing.is_some_and(|(yours, _)| yours) {
        "~/.config/cyclops/config.toml"
    } else {
        ".cyclops/config.toml"
    };
    let writes = canvas.pane(bottom, &format!("Writes to {file}"), false);
    let mut y = writes.y;
    let marker = if editor.existing.is_some() { "~" } else { "+" };
    if editor.existing.is_none() {
        canvas.text(writes.x + 1, y, "+ [[alerts]]", Token::Fg, writes);
        y += 1;
    }
    for (name, raw) in editor.fields() {
        let shown = match raw {
            RawValue::Text(text) => format!("\"{text}\""),
            RawValue::Bool(flag) => flag.to_string(),
            RawValue::Integer(number) => number.to_string(),
            RawValue::List(items) => {
                let items: Vec<String> = items
                    .iter()
                    .filter_map(|item| match item {
                        RawValue::Text(text) => Some(format!("\"{text}\"")),
                        _ => None,
                    })
                    .collect();
                format!("[{}]", items.join(", "))
            }
            RawValue::Other => String::new(),
        };
        canvas.text(
            writes.x + 1,
            y,
            &format!("{marker} {name} = {shown}"),
            Token::Fg,
            writes,
        );
        y += 1;
    }
    let note = "Nothing is saved until :w. Existing lines and comments stay as they are.";
    canvas.text(writes.x + 1, y + 1, note, Token::Dim, writes);
}

/// A relation, in the line style of its confidence, with its head before the target.
fn draw_route(canvas: &mut Canvas<'_>, route: &Route, clip: Rect) {
    let (across, down, token) = if route.in_cycle {
        ('━', '┃', Token::H3)
    } else {
        match route.confidence {
            Confidence::Exact => ('─', '│', Token::Line),
            Confidence::Inferred => ('╌', '╎', Token::Line),
            Confidence::Ambiguous => ('┄', '┆', Token::Line),
        }
    };
    let (sx, sy) = route.start;
    let (ex, ey) = route.end;
    let middle = u16::midpoint(sx, ex);
    let line = |canvas: &mut Canvas<'_>, from: u16, to: u16, y: u16| {
        for x in from.min(to)..=from.max(to) {
            canvas.text(x, y, &across.to_string(), token, clip);
        }
    };
    line(canvas, sx, middle, sy);
    for y in sy.min(ey)..=sy.max(ey) {
        canvas.text(middle, y, &down.to_string(), token, clip);
    }
    line(canvas, middle, ex, ey);
    canvas.text(ex, ey, &route.head.to_string(), Token::Fg, clip);
}

/// A node: a box with its name and value, or a class box with its members.
fn draw_card(
    app: &App,
    canvas: &mut Canvas<'_>,
    card: &Card,
    selected: Option<&SymbolId>,
    metric: MetricId,
    marks: &HashMap<SymbolId, String>,
) {
    let focused = app.focus() == Some(&card.id);
    let (border, glyphs, divider) = if focused {
        (Token::Acc, ['┏', '┓', '┗', '┛', '━', '┃'], ('┠', '─', '┨'))
    } else {
        (Token::Line, ['╭', '╮', '╰', '╯', '─', '│'], ('├', '─', '┤'))
    };
    canvas.fill(inner(card.area), Token::Fg);
    canvas.frame(card.area, border, glyphs);
    let inside = inner(card.area);
    let mut y = inside.y;
    let mut rows = card.rows.iter().peekable();
    if let Some(BoxRow::Stereotype(stereotype)) = rows.peek() {
        canvas.text(
            inside.x + 1,
            y,
            &format!("«{stereotype}»"),
            Token::Dim,
            inside,
        );
        y += 1;
        rows.next();
    }
    let boxed = app
        .symbol(&card.id)
        .is_some_and(|symbol| symbol.kind.is_type());
    let name = app.symbol(&card.id).map_or_else(
        || card.id.to_string(),
        |symbol| {
            if boxed {
                symbol.name.clone()
            } else {
                short_subject(&Subject::Symbol(symbol.id.clone()))
            }
        },
    );
    let is_selected = selected == Some(&card.id);
    let mut label = if is_selected {
        format!("▸{name}")
    } else {
        name
    };
    if let Some(mark) = marks.get(&card.id) {
        label = format!("{label} {mark}");
    }
    let name_row = Rect {
        y,
        height: 1,
        ..inside
    };
    let token = if focused { Token::Accb } else { Token::B };
    // The marker of the selection sits in the gap before the name.
    let name_x = if is_selected { inside.x } else { inside.x + 1 };
    canvas.text(name_x, y, &label, token, name_row);
    let value = app.shown_value(&card.id, metric);
    if boxed {
        canvas.text_right(inside.x + inside.width, y, &value, Token::Fg, name_row);
    } else {
        let overlay = format!("{} {value}", metric_label(metric));
        canvas.text(inside.x + 1, y + 1, &overlay, Token::Dim, inside);
    }
    if is_selected {
        canvas.paint(name_row, Token::Sel);
    }
    canvas.hit(name_row, Target::Symbol(card.id.clone()));
    y += 1;
    draw_box_rows(canvas, card, rows, y, selected, metric, (border, divider));
}

/// Lists entries of a popup, each one clickable, with the current one marked.
fn popup_entries(canvas: &mut Canvas<'_>, area: Rect, entries: &[(String, String)], cursor: usize) {
    for (index, (label, right)) in entries.iter().enumerate() {
        let y = area.y + 2 + u16::try_from(index).unwrap_or(u16::MAX);
        if y + 1 >= area.y + area.height {
            break;
        }
        let row = Rect {
            x: area.x + 1,
            y,
            width: area.width.saturating_sub(2),
            height: 1,
        };
        let token = if index == cursor {
            Token::Pops
        } else {
            Token::Pop
        };
        canvas.fill(row, token);
        canvas.text(row.x + 1, y, label, token, row);
        canvas.text_right(row.x + row.width - 1, y, right, Token::Popk, row);
        canvas.hit(row, Target::Entry(label.clone()));
    }
}

fn relation_picker(
    canvas: &mut Canvas<'_>,
    body: Rect,
    typed: &str,
    ticked: &[RelationKind],
    cursor: usize,
) {
    let choices = relation_choices(typed);
    let height = u16::try_from(choices.len()).unwrap_or(0) + 7;
    let area = Rect {
        x: body.x + body.width.saturating_sub(60) / 2,
        y: body.y + 6.min(body.height.saturating_sub(height)),
        width: 60.min(body.width),
        height: height.min(body.height),
    };
    popup_frame(canvas, area, "");
    popup_prompt(canvas, area, "Relations to draw", typed);
    for (index, kind) in choices.iter().enumerate() {
        let y = area.y + 4 + u16::try_from(index).unwrap_or(0);
        let row = Rect {
            x: area.x + 1,
            y,
            width: area.width.saturating_sub(2),
            height: 1,
        };
        let token = if index == cursor {
            Token::Pops
        } else {
            Token::Pop
        };
        canvas.fill(row, token);
        canvas.hit(row, Target::Entry(kind.name().to_owned()));
        let tick = if ticked.contains(kind) { "[x]" } else { "[ ]" };
        let line = format!("{tick} {} {}", relation_glyph(*kind), kind.name());
        canvas.text(area.x + 2, y, &line, token, row);
        canvas.text(area.x + 20, y, relation_meaning(*kind), Token::Popd, row);
    }
    let y = area.y + area.height.saturating_sub(2);
    let end = canvas.text(area.x + 2, y, "Type to narrow.   ", Token::Popd, area);
    let end = canvas.text(end, y, "Space", Token::Popk, area);
    let end = canvas.text(end, y, " tick   ", Token::Popd, area);
    let end = canvas.text(end, y, "Enter", Token::Popk, area);
    canvas.text(end, y, " apply", Token::Popd, area);
}

/// The head a relation of this kind ends in, as the legend shows it.
const fn relation_glyph(kind: RelationKind) -> &'static str {
    match kind {
        RelationKind::Implements => "○",
        RelationKind::Inherits => "▷",
        RelationKind::Composes => "◆",
        _ => ">",
    }
}

/// What a relation of this kind says.
const fn relation_meaning(kind: RelationKind) -> &'static str {
    match kind {
        RelationKind::Imports => "A module depends on another",
        RelationKind::Implements => "A type implements an interface",
        RelationKind::Inherits => "A class extends a class",
        RelationKind::Composes => "A type has a field of another",
        RelationKind::Uses => "A function mentions a type",
        RelationKind::Calls => "A function calls another",
    }
}

fn more_nodes(app: &App, canvas: &mut Canvas<'_>, body: Rect, hidden: &[SymbolId], cursor: usize) {
    let scene = app.scene();
    let card = scene.as_ref().and_then(|(_, scene)| {
        scene
            .more
            .iter()
            .find(|more| {
                more.hidden.len() == hidden.len()
                    && hidden.iter().all(|id| more.hidden.contains(id))
            })
            .cloned()
    });
    let height = u16::try_from(hidden.len())
        .unwrap_or(u16::MAX)
        .saturating_add(6);
    // Beside the card it opens from, a little above it, or in the middle without one.
    let (x, y) = card.as_ref().map_or_else(
        || (body.x + body.width.saturating_sub(50) / 2, body.y + 1),
        |card| {
            (
                card.area.x + card.area.width + 1,
                card.area.y.saturating_sub(3),
            )
        },
    );
    let area = Rect {
        x: x.min((body.x + body.width).saturating_sub(50)),
        y: y.min((body.y + body.height).saturating_sub(height)),
        width: 50.min(body.width),
        height: height.min(body.height),
    };
    popup_frame(canvas, area, "");
    let focus = app.focus();
    let focus_name = focus
        .and_then(|focus| app.symbol(focus))
        .map(|symbol| symbol.name.clone())
        .unwrap_or_default();
    let edges = scene
        .as_ref()
        .map(|(drawing, _)| drawing.view.edges.clone())
        .unwrap_or_default();
    let depends = hidden.iter().any(|id| {
        edges
            .iter()
            .any(|edge| &edge.from == id && Some(&edge.to) == focus)
    });
    let title = if depends {
        format!("{} more depend on {focus_name}", hidden.len())
    } else {
        format!("{focus_name} depends on {} more", hidden.len())
    };
    canvas.text(area.x + 2, area.y + 1, &title, Token::Popb, area);
    let note = "Worst first. Enter selects it in the diagram.";
    canvas.text(area.x + 2, area.y + 2, note, Token::Popd, area);
    let metric = app.lens().metric;
    for (index, id) in hidden.iter().enumerate() {
        let y = area.y + 4 + u16::try_from(index).unwrap_or(u16::MAX);
        let row = Rect {
            x: area.x + 1,
            y,
            width: area.width.saturating_sub(2),
            height: 1,
        };
        let token = if index == cursor {
            Token::Pops
        } else {
            Token::Pop
        };
        canvas.fill(row, token);
        let confidence = edges
            .iter()
            .find(|edge| {
                (&edge.from == id && Some(&edge.to) == focus)
                    || (&edge.to == id && Some(&edge.from) == focus)
            })
            .map(|edge| edge.confidence);
        let guess = if confidence.is_some_and(|sure| sure != Confidence::Exact) {
            " ~"
        } else {
            ""
        };
        let name = format!("{}{guess}", short_subject(&Subject::Symbol(id.clone())));
        canvas.text(area.x + 2, y, &name, token, row);
        canvas.text_right(area.x + 33, y, &app.shown_value(id, metric), token, row);
        if let Some(confidence) = confidence {
            canvas.text_right(
                area.x + area.width - 2,
                y,
                confidence.name(),
                Token::Popd,
                row,
            );
        }
        canvas.hit(row, Target::Entry(id.to_string()));
    }
}

/// The tiles of a `+N small` tile, largest first.
fn small_tiles(
    app: &App,
    canvas: &mut Canvas<'_>,
    body: Rect,
    tiles: &[SymbolId],
    cursor: usize,
    packages: bool,
) {
    let height = u16::try_from(tiles.len())
        .unwrap_or(u16::MAX)
        .saturating_add(6);
    let area = Rect {
        x: body.x + body.width.saturating_sub(50) / 2,
        y: body.y + 1,
        width: 50.min(body.width),
        height: height.min(body.height),
    };
    popup_frame(canvas, area, "");
    let what = if packages { "packages" } else { "tiles" };
    let title = format!("{} small {what}", tiles.len());
    canvas.text(area.x + 2, area.y + 1, &title, Token::Popb, area);
    let note = "Too small to draw. Enter zooms into one.";
    canvas.text(area.x + 2, area.y + 2, note, Token::Popd, area);
    let metric = app.lens().metric;
    let first = cursor.saturating_sub(usize::from(area.height.saturating_sub(7)));
    for (index, id) in tiles.iter().enumerate().skip(first) {
        let y = area.y + 4 + u16::try_from(index - first).unwrap_or(u16::MAX);
        if y + 2 > area.y + area.height {
            break;
        }
        let row = Rect {
            x: area.x + 1,
            y,
            width: area.width.saturating_sub(2),
            height: 1,
        };
        let token = if index == cursor {
            Token::Pops
        } else {
            Token::Pop
        };
        canvas.fill(row, token);
        canvas.text(area.x + 2, y, id.as_str(), token, row);
        canvas.text_right(
            area.x + area.width - 2,
            y,
            &app.shown_value(id, metric),
            token,
            row,
        );
        canvas.hit(row, Target::Entry(id.to_string()));
    }
}

fn context_menu(app: &App, canvas: &mut Canvas<'_>, body: Rect, cursor: usize) {
    let name = app
        .selected()
        .and_then(|id| app.symbol(&id).map(|symbol| symbol.name.clone()))
        .unwrap_or_default();
    let height = (u16::try_from(CONTEXT_MENU.len()).unwrap_or(0) + 3).min(body.height);
    let width = 38.min(body.width);
    // Where it was clicked, kept on screen, or near the top for the keyboard.
    let (x, y) = app
        .context_at()
        .unwrap_or((body.x + 20.min(body.width / 4), body.y + 3));
    let area = Rect {
        x: x.min((body.x + body.width).saturating_sub(width)),
        y: y.min((body.y + body.height).saturating_sub(height)),
        width,
        height,
    };
    popup_frame(canvas, area, "");
    canvas.text(area.x + 2, area.y + 1, &name, Token::Popb, area);
    let entries: Vec<(String, String)> = CONTEXT_MENU
        .iter()
        .map(|(label, keys)| ((*label).to_owned(), (*keys).to_owned()))
        .collect();
    popup_entries(
        canvas,
        Rect {
            y: area.y.saturating_sub(0),
            ..area
        },
        &entries,
        cursor,
    );
}

/// The rows of a class box below its name: dividers, members and `+N more`.
fn draw_box_rows<'a>(
    canvas: &mut Canvas<'_>,
    card: &Card,
    rows: impl Iterator<Item = &'a BoxRow>,
    mut y: u16,
    selected: Option<&SymbolId>,
    metric: MetricId,
    style: (Token, (char, char, char)),
) {
    let (border, divider) = style;
    let inside = inner(card.area);
    for row in rows {
        let line = Rect {
            y,
            height: 1,
            ..inside
        };
        match row {
            BoxRow::Divider => {
                let (left, across, right) = divider;
                let text = format!(
                    "{left}{}{right}",
                    across
                        .to_string()
                        .repeat(usize::from(card.area.width.saturating_sub(2)))
                );
                canvas.text(card.area.x, y, &text, border, card.area);
            }
            BoxRow::Member {
                id,
                text,
                value,
                alerted,
            } => {
                let mark = if *alerted { " !" } else { "" };
                let pointer = if selected == Some(id) { "▸" } else { " " };
                let shown = value.map_or_else(
                    || "n/a".to_owned(),
                    |value| crate::state::shown(metric, value),
                );
                // The name gives way to the value, and keeps a space before it.
                let room = inside.width.saturating_sub(width(&shown) + 1);
                canvas.text(
                    inside.x,
                    y,
                    &fit(&format!("{pointer}{text}{mark}"), room),
                    Token::Fg,
                    line,
                );
                canvas.text_right(inside.x + inside.width, y, &shown, Token::Fg, line);
                if selected == Some(id) {
                    canvas.paint(line, Token::Sel);
                }
                canvas.hit(line, Target::Symbol(id.clone()));
            }
            BoxRow::More(count) => {
                canvas.text(
                    inside.x + 1,
                    y,
                    &format!("+{count} more"),
                    Token::Faint,
                    line,
                );
            }
            BoxRow::Stereotype(_) => {}
        }
        y += 1;
    }
}

/// The kind of the focus as row 1 names it: `project` for the top, and otherwise the
/// kind, such as `module` or `method`.
fn kind_name(app: &App, id: &SymbolId) -> String {
    app.symbol(id).map_or_else(String::new, |symbol| {
        if symbol.parent.is_none() {
            "project".to_owned()
        } else {
            symbol.kind.name().to_owned()
        }
    })
}

/// The last two parts of an id, as a short name.
fn short_id(id: &str) -> String {
    let parts: Vec<&str> = id.split("::").collect();
    let keep = parts.len().saturating_sub(2);
    parts
        .get(keep..)
        .map(|parts| parts.join("::"))
        .unwrap_or_default()
}

/// A duration as the status line says it, such as `3m 12s`.
fn elapsed(millis: u64) -> String {
    let seconds = millis / 1000;
    if seconds < 60 {
        format!("{seconds}s")
    } else {
        format!("{}m {}s", seconds / 60, seconds % 60)
    }
}

/// The highest functions so far, before the analysis is done.
fn partial_ranking(canvas: &mut Canvas<'_>, inside: Rect, partial: &Partial, metric: MetricId) {
    canvas.text(inside.x + 1, inside.y, "symbol", Token::Faint, inside);
    canvas.text(inside.x + 53, inside.y, "kind", Token::Faint, inside);
    let right = inside.x + inside.width;
    canvas.text_right(
        right - 3,
        inside.y,
        metric_label(metric),
        Token::Faint,
        inside,
    );
    let mut ranked: Vec<(&str, f64)> = partial
        .functions
        .iter()
        .filter_map(|function| {
            let value = function.values.iter().find(|(one, _)| *one == metric)?.1;
            Some((function.name.as_str(), value))
        })
        .collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    for (index, (name, value)) in ranked.iter().take(5).enumerate() {
        let y = inside.y + 1 + u16::try_from(index).unwrap_or_default();
        canvas.text(inside.x + 1, y, name, Token::Fg, inside);
        canvas.text(inside.x + 53, y, "function", Token::Fg, inside);
        let shown = crate::state::shown(metric, *value);
        canvas.text_right(right - 3, y, &shown, Token::Fg, inside);
    }
    let note = format!("From {} files. This list will change.", partial.files);
    canvas.text(inside.x + 1, inside.y + 7, &note, Token::Dim, inside);
}

fn status_limits(app: &App, canvas: &mut Canvas<'_>, area: Rect) {
    let inside = canvas.pane(area, "Known limits", false);
    canvas.text(
        inside.x + 1,
        inside.y,
        "Logic inside macros is undercounted, even with",
        Token::Dim,
        inside,
    );
    canvas.text(
        inside.x + 1,
        inside.y + 1,
        "precise analysis.",
        Token::Dim,
        inside,
    );
    let Some(analysis) = app.analysis() else {
        return;
    };
    let unavailable: Vec<String> = analysis
        .unavailable()
        .iter()
        .map(ToString::to_string)
        .collect();
    if unavailable.is_empty() {
        canvas.text(
            inside.x + 1,
            inside.y + 3,
            "Metrics unavailable: none",
            Token::Fg,
            inside,
        );
        let note = "Without git, churn and hotspot would be listed here.";
        canvas.text(inside.x + 1, inside.y + 4, note, Token::Dim, inside);
    } else {
        canvas.text(
            inside.x + 1,
            inside.y + 3,
            "Metrics unavailable:",
            Token::Fg,
            inside,
        );
        for (row, line) in unavailable.iter().enumerate() {
            let y = inside.y + 4 + u16::try_from(row).unwrap_or(u16::MAX);
            canvas.text(inside.x + 1, y, line, Token::Fg, inside);
        }
    }
}

fn status_tests(app: &App, canvas: &mut Canvas<'_>, area: Rect) {
    let inside = canvas.pane(area, "Test code", false);
    let Some(analysis) = app.analysis() else {
        return;
    };
    let test_files: Vec<_> = analysis
        .files()
        .iter()
        .filter(|file| file.role == CodeRole::Test || file.test_lines.code > 0)
        .collect();
    let lines: u32 = test_files.iter().map(|file| file.test_lines.code).sum();
    let (state, hint, note) = if app.lens().tests {
        (
            "Included",
            " leave test code out",
            "Counted in every metric, and shown in every view.",
        )
    } else {
        (
            "Left out",
            " include test code",
            "Hidden from every view, and counted in no metric.",
        )
    };
    let x = canvas.text(inside.x + 1, inside.y, state, Token::B, inside);
    let counts = format!(
        " {}, {}",
        plural(test_files.len(), "file"),
        plural(usize::try_from(lines).unwrap_or(0), "line")
    );
    canvas.text(x, inside.y, &counts, Token::Fg, inside);
    canvas.text(inside.x + 1, inside.y + 2, note, Token::Dim, inside);
    canvas.text(
        inside.x + 1,
        inside.y + 3,
        "Switching never re-parses anything.",
        Token::Dim,
        inside,
    );
    let x = canvas.text(inside.x + 1, inside.y + 5, "Space t", Token::Acc, inside);
    canvas.text(x, inside.y + 5, hint, Token::Fg, inside);
}

fn status_index(app: &App, canvas: &mut Canvas<'_>, area: Rect) {
    let inside = canvas.pane(area, "Semantic index", false);
    let current = app
        .analysis()
        .and_then(|analysis| analysis.precise().current);
    let lines: Vec<String> = match current {
        None => vec![
            "No index yet, so calls are matched by name.".to_owned(),
            String::new(),
            "Once indexed, this shows which files changed since.".to_owned(),
            "Those files fall back to syntax relations and are".to_owned(),
            "marked * everywhere.".to_owned(),
        ],
        Some(true) => vec!["The index matches the code.".to_owned()],
        Some(false) => {
            let changed = app
                .analysis()
                .map_or(0, |analysis| analysis.precise().changed);
            vec![
                format!("{} changed since the index.", plural(changed, "file")),
                "Their relations are back to the syntax layer, and".to_owned(),
                "are marked * everywhere until the next index.".to_owned(),
            ]
        }
    };
    // An index names the commit it was made at, when the history says.
    let commit = app
        .analysis()
        .and_then(|analysis| analysis.precise().commit.as_ref())
        .filter(|_| current.is_some());
    let lines: Vec<String> = commit
        .map(|commit| {
            vec![
                format!("Indexed at {} {}", commit.id, commit.subject),
                String::new(),
            ]
        })
        .unwrap_or_default()
        .into_iter()
        .chain(lines)
        .collect();
    for (row, line) in lines.iter().enumerate() {
        let y = inside.y + 1 + u16::try_from(row).unwrap_or(u16::MAX);
        let token = if row == 0 { Token::Fg } else { Token::Dim };
        canvas.text(inside.x + 1, y, line, token, inside);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_member_keeps_a_space_before_its_value() {
        let area = Rect::new(0, 0, 24, 4);
        let mut buffer = Buffer::empty(area);
        let id = SymbolId::new("Cli::ignored_by_git");
        let row = BoxRow::Member {
            id,
            text: "+ ignored_by_git()".to_owned(),
            value: None,
            alerted: false,
        };
        let card = Card {
            id: SymbolId::new("Cli"),
            column: 0,
            area,
            rows: vec![row],
        };
        let mut canvas = Canvas::new(&mut buffer, crate::state::Theme::NoColor);
        let style = (Token::Line, ('├', '─', '┤'));
        draw_box_rows(
            &mut canvas,
            &card,
            card.rows.iter(),
            1,
            None,
            MetricId::Lcom,
            style,
        );
        let text = crate::canvas::screen_text(&buffer);
        let line = text.lines().nth(1).unwrap_or_default();
        assert!(line.ends_with("+ ignored_by_git… n/a"), "{line:?}");
    }

    #[test]
    fn a_spread_draws_each_range_against_the_fullest() {
        // The design's cyclomatic spread: the fullest range is the tallest glyph, an
        // empty range is blank, and any count draws at least the lowest glyph.
        assert_eq!(sparkline(&[70, 55, 25, 15, 5, 4, 0, 1]), "▇▆▃▂▁▁ ▁");
        assert_eq!(sparkline(&[10, 20, 30, 50, 70, 50, 30, 20]), "▁▂▃▅▇▅▃▂");
        assert_eq!(sparkline(&[0; 8]), "        ");
    }
}

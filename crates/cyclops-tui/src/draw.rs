//! What the interface looks like: the shell, each view, the inspector and the popups.
//! See `specs/tui/design/`.

use crate::app::{App, MIN_COLUMNS, MIN_ROWS, Running, SIDE_BY_SIDE, hints, picked, space_menu};
use crate::app::{CONTEXT_MENU, relation_choices};
use crate::canvas::inner;
use crate::canvas::{Canvas, fit, width};
use crate::editor::{FIELDS, RuleEditor};
use crate::scene::{BoxRow, Card, Route, routes};
use crate::state::Pose;
use crate::state::{
    OVERVIEW_METRICS, Pane, Popup, PromptKind, Section, Target, View, metric_label,
};
use crate::tokens::Token;
use cyclops_application::analysis::ProjectAnalysis;
use cyclops_application::diagram::Level;
use cyclops_application::ports::Step;
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
    canvas.text(area.x + 2, area.y, "◉", Token::Acc, area);
    canvas.text(area.x + 4, area.y, "cyclops", Token::B, area);
    let mut x = area.x + 12;
    for view in View::ALL {
        let name = view.name();
        let start = x;
        if view == app.view() {
            x = canvas.text(x, area.y, name, Token::On, area);
        } else {
            let mut letters = name.chars();
            let first: String = letters.next().map(String::from).unwrap_or_default();
            x = canvas.text(x, area.y, &first, Token::Key, area);
            x = canvas.text(x, area.y, letters.as_str(), Token::Dim, area);
        }
        canvas.hit(
            Rect {
                x: start,
                y: area.y,
                width: x - start,
                height: 1,
            },
            Target::Tab(view),
        );
        x += 2;
    }
    if let Some(analysis) = app.analysis() {
        let lines: u32 = analysis
            .files()
            .iter()
            .map(|file| file.lines.code + file.lines.comment + file.lines.blank)
            .sum();
        let project = app
            .focus()
            .and_then(|focus| app.symbol(focus))
            .map(|symbol| root_name(app, symbol))
            .unwrap_or_default();
        let summary = format!(
            "{project} · {} files · {} lines",
            grouped(analysis.files().len()),
            grouped(usize::try_from(lines).unwrap_or_default())
        );
        let right = area.x + area.width - 1;
        if right > x + width(&summary) {
            canvas.text_right(right, area.y, &summary, Token::Dim, area);
        }
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
    let parts = app.breadcrumb();
    let ids = app.breadcrumb_ids();
    let mut x = area.x + 1;
    let last = parts.len().saturating_sub(1);
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            x = canvas.text(x, y, " › ", Token::Faint, area);
        }
        let token = if index == last { Token::B } else { Token::Fg };
        let start = x;
        x = canvas.text(x, y, part, token, area);
        if let Some(id) = ids.get(index) {
            canvas.hit(
                Rect::new(start, y, x - start, 1),
                Target::Breadcrumb(id.clone()),
            );
        }
    }
    let kind = app
        .focus()
        .and_then(|focus| app.symbol(focus))
        .map(|symbol| {
            if symbol.parent.is_none() {
                "project".to_owned()
            } else {
                symbol.kind.name().to_owned()
            }
        })
        .unwrap_or_default();
    let (position, length) = app.trail();
    let mut right = kind;
    if length > 1 {
        right = format!("{right} · trail {position}/{length}");
    }
    let selected = app.selected();
    if let Some(selected) = selected.filter(|selected| Some(selected) != app.focus()) {
        let name = app
            .symbol(&selected)
            .map(|symbol| symbol.name.clone())
            .unwrap_or_default();
        right = format!("{right} · selected {name}");
    }
    canvas.text_right(area.x + area.width - 1, y, &right, Token::Faint, area);
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
    let mode = format!(" {} ", app.mode().name());
    let mode_token = if app.mode() == crate::state::Mode::Normal {
        Token::Mode
    } else {
        Token::On
    };
    let mut x = canvas.text(area.x, y, &mode, mode_token, line);
    let lens = app.lens();
    let filter = lens
        .filter
        .as_ref()
        .map_or("none", |filter| filter.text.as_str());
    let tests = if lens.tests { "on" } else { "off" };
    let layer = if app.precise_layer() {
        "precise"
    } else {
        "syntax"
    };
    for (label, value) in [
        ("metric", app.metric_label()),
        ("filter", filter),
        ("tests", tests),
        ("layer", layer),
    ] {
        let start = x + 2;
        x = canvas.text(x + 2, y, label, Token::Chip, line);
        x = canvas.text(x + 1, y, value, Token::Chipa, line);
        if let Some(chip) = ["metric", "filter", "tests"]
            .into_iter()
            .find(|chip| *chip == label)
        {
            canvas.hit(Rect::new(start, y, x - start, 1), Target::Chip(chip));
        }
    }
    let mut parts: Vec<(String, Token)> = Vec::new();
    if let Some(analysis) = app.analysis() {
        let alerts = analysis.alerts().len();
        parts.push((plural(alerts, "alert"), Token::Chiph));
        parts.push((plural(analysis.problems().len(), "problem"), Token::Chip));
    } else if app.failure().is_some() {
        parts.push((plural(1, "problem"), Token::Chip));
    }
    let freshness = match app.running_analysis() {
        Some(running) if running.precise => {
            let elapsed = app.now().saturating_sub(running.since) / 1000;
            let progress = running
                .percent
                .map_or_else(|| format!("{elapsed}s"), |percent| format!("{percent}%"));
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
    };
    parts.push(freshness);
    let total: u16 = parts.iter().map(|(text, _)| width(text) + 2).sum();
    let mut x = (area.x + area.width).saturating_sub(total).max(x + 2);
    for (text, token) in parts {
        let start = x;
        x = canvas.text(x, y, &text, token, line) + 2;
        let chip = if text.contains("alert") {
            Some("alerts")
        } else if text.contains("problem") {
            Some("problems")
        } else {
            None
        };
        if let Some(chip) = chip {
            canvas.hit(Rect::new(start, y, x - start - 2, 1), Target::Chip(chip));
        }
    }
}

fn plural(count: usize, word: &str) -> String {
    if count == 1 {
        format!("1 {word}")
    } else {
        format!("{count} {word}s")
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
            ("< >", "depth"),
            ("c", "center"),
            ("r", "relations"),
            ("K", "inspector"),
        ],
        View::Map => &[
            ("hjkl", "move"),
            ("Enter", "zoom in"),
            ("-", "zoom out"),
            ("K", "inspector"),
            ("M", "metric"),
        ],
        View::Find => &[
            ("f", "filter"),
            ("s", "sort"),
            ("Enter", "open"),
            ("yy", "copy"),
            ("M", "metric"),
        ],
        View::Alerts => &[
            ("e", "edit"),
            ("x", "disable"),
            ("v", "group"),
            ("A", "new rule"),
        ],
        View::Status => &[
            ("]p", "next problem"),
            ("Space p", "precise"),
            ("Space t", "test code"),
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
        for (key, meaning) in view_hints(app.view()) {
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
        SymbolKind::Package => Some(Scope::Project),
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

/// How many whole steps of `limit` fit in `share` of it, a share being from 0 to 1.
fn steps(share: f64, limit: u16) -> u16 {
    let target = share.clamp(0.0, 1.0) * f64::from(limit);
    (0..=limit)
        .rev()
        .find(|step| f64::from(*step) <= target + 0.5)
        .unwrap_or(0)
}

/// A bar of `cells` cells, filled in proportion.
fn bar(value: f64, highest: f64, cells: u16) -> (String, String) {
    let share = if highest > 0.0 { value / highest } else { 0.0 };
    let halves = steps(share, cells * 2);
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
    let heading = format!("{} (highest inside)", metric_label(metric));
    canvas.text(inside.x + 1, inside.y, "name", Token::Faint, inside);
    let bar_cells: u16 = 10;
    let value_right = inside.x + inside.width.saturating_sub(bar_cells + 2);
    canvas.text_right(value_right, inside.y, &heading, Token::Faint, inside);
    let rows = app.tree();
    let marks = app.tree_marks();
    let highest = rows
        .iter()
        .filter_map(|row| analysis.value(&Subject::Symbol(row.symbol.id.clone()), metric))
        .fold(0.0, f64::max);
    let room = usize::from(inside.height.saturating_sub(1));
    let selected = app
        .focus()
        .and_then(|focus| rows.iter().position(|row| &row.symbol.id == focus));
    let first = selected.map_or(0, |selected| {
        selected.saturating_sub(room.saturating_sub(1))
    });
    for (index, row) in rows.iter().enumerate().skip(first).take(room) {
        let y = inside.y + 1 + u16::try_from(index - first).unwrap_or_default();
        let line = Rect {
            x: inside.x,
            y,
            width: inside.width,
            height: 1,
        };
        let indent = u16::try_from(row.depth * 2).unwrap_or(u16::MAX);
        let arrow = match row.open {
            Some(true) => "▾ ",
            Some(false) => "▸ ",
            None => "  ",
        };
        let x = canvas.text(inside.x + 1 + indent, y, arrow, Token::Dim, line);
        let mark_column = value_right.saturating_sub(10);
        let name_room = Rect {
            x: line.x,
            width: mark_column.saturating_sub(line.x + 1),
            ..line
        };
        canvas.text(x, y, &row.symbol.name, Token::Fg, name_room);
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
        canvas.text_right(value_right, y, &value, token, line);
        if let Some(number) = analysis.value(&Subject::Symbol(row.symbol.id.clone()), metric) {
            let (filled, rest) = bar(number, highest, bar_cells);
            let x = canvas.text(value_right + 1, y, &filled, token, line);
            canvas.text(x, y, &rest, Token::Track, line);
        }
        canvas.hit(line, Target::Symbol(row.symbol.id.clone()));
        if Some(index) == selected {
            canvas.paint(line, Token::Sel);
        }
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
    canvas.text(inside.x + 1, inside.y, &symbol.name, Token::B, inside);
    let right = inside.x + inside.width - 1;
    canvas.text_right(right, inside.y, symbol.kind.name(), Token::Dim, inside);
    if let Some(location) = &symbol.location {
        let file = location.file.as_str();
        canvas.text(inside.x + 1, inside.y + 1, file, Token::Dim, inside);
    }
    let mut x = inside.x + 2;
    let open = app.inspector().unwrap_or(Section::Metrics);
    for section in Section::ALL {
        let token = if section == open {
            Token::On
        } else {
            Token::Dim
        };
        x = canvas.text(x, inside.y + 3, section.name(), token, inside) + 3;
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

fn inspector_metrics(
    app: &App,
    analysis: &ProjectAnalysis,
    symbol: &Symbol,
    canvas: &mut Canvas<'_>,
    body: Rect,
) {
    canvas.text(body.x + 1, body.y, "metric", Token::Faint, body);
    canvas.text(body.x + 15, body.y, "value", Token::Faint, body);
    canvas.text(body.x + 22, body.y, "among", Token::Faint, body);
    let subject = Subject::Symbol(symbol.id.clone());
    let mut y = body.y + 1;
    if let Some(scope) = scope_of(symbol) {
        for metric in MetricId::ALL {
            if !metric.descriptor().applies_to(scope) {
                continue;
            }
            let value = analysis.value(&subject, metric);
            let shown = value.map_or_else(
                || "n/a".to_owned(),
                |value| metric.descriptor().format(value),
            );
            let token = value_token(analysis, symbol, metric);
            canvas.text(body.x + 1, y, metric_label(metric), Token::Fg, body);
            canvas.text_right(body.x + 20, y, &shown, token, body);
            if let Some(rank) = analysis.rank(&subject, metric) {
                let rank = format!("#{} of {}", rank.position, rank.of);
                canvas.text(body.x + 22, y, &rank, Token::Dim, body);
            }
            y += 1;
        }
    }
    y += 1;
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
        let counts = format!(
            "{errors} error   {} warning   of {} in the project",
            inside_it.len() - errors,
            analysis.alerts().len()
        );
        canvas.text(body.x + 1, y, "Alerts inside", Token::B, body);
        canvas.text(body.x + 1, y + 1, &counts, Token::Fg, body);
        y += 3;
    }
    if let Some(path) = analysis.cycle_path(&symbol.id) {
        let names: Vec<String> = path
            .iter()
            .map(|(from, _, _)| {
                app.symbol(from)
                    .map_or_else(|| from.to_string(), |symbol| symbol.name.clone())
            })
            .collect();
        let line = format!("@ {}   size {}", names.join(", "), names.len());
        canvas.text(body.x + 1, y, "Cycle", Token::B, body);
        canvas.text(body.x + 1, y + 1, &line, Token::H2, body);
        y += 3;
    }
    let mut counts = [0_usize; 3];
    for relation in analysis.view().relations() {
        if (relation.from == symbol.id || relation.to == symbol.id)
            && let Some(count) = counts.get_mut(confidence_index(relation.confidence))
        {
            *count += 1;
        }
    }
    let [exact, inferred, ambiguous] = counts;
    let line = format!("{exact} exact   {inferred} inferred   {ambiguous} ambiguous ~");
    canvas.text(body.x + 1, y, "Relations", Token::B, body);
    canvas.text(body.x + 1, y + 1, &line, Token::Fg, body);
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
    let lines = why_lines(app, analysis, symbol, metric);
    for (row, line) in lines.iter().enumerate() {
        let y = body.y + 2 + u16::try_from(row).unwrap_or(u16::MAX);
        canvas.text(body.x + 1, y, line, Token::Fg, body);
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
    let impact = analysis.impact(&symbol.id);
    let all: Vec<&(SymbolId, Confidence)> = impact.all().collect();
    if all.is_empty() {
        canvas.text(
            body.x + 1,
            body.y,
            "Nothing depends on this.",
            Token::Dim,
            body,
        );
        return;
    }
    let view = analysis.view();
    let mut modules: Vec<SymbolId> = all
        .iter()
        .filter_map(|(id, _)| view.module_of(id).map(|module| module.id.clone()))
        .collect();
    modules.sort();
    modules.dedup();
    let total = view
        .symbols()
        .filter(|symbol| symbol.kind == SymbolKind::Module && symbol.location.is_some())
        .count();
    let reach = format!("Reach {}   {} of {total} modules", all.len(), modules.len());
    canvas.text(body.x + 1, body.y, &reach, Token::B, body);
    let guesses = all
        .iter()
        .filter(|(_, confidence)| *confidence != Confidence::Exact)
        .count();
    let sure = match guesses {
        0 => "All of these are exact.".to_owned(),
        1 => "1 of these is a guess.".to_owned(),
        many => format!("{many} of these are guesses."),
    };
    canvas.text(body.x + 1, body.y + 1, &sure, Token::Dim, body);
    let mut y = body.y + 3;
    for (distance, level) in impact.levels.iter().enumerate() {
        canvas.text(
            body.x + 1,
            y,
            &format!("distance {}", distance + 1),
            Token::Faint,
            body,
        );
        y += 1;
        for (id, confidence) in level {
            let line = format!("  {id}  {}", confidence.name());
            canvas.text(body.x + 1, y, &line, Token::Fg, body);
            y += 1;
        }
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
    let right = Rect {
        x: body.x + left_width,
        width: body.width - left_width,
        ..body
    };
    let metrics_height = 18.min(right.height / 2);
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
        height: 11.min(area.height),
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
    let Some(analysis) = app.analysis() else {
        let files = app.running_analysis().map_or(0, |running| running.files.1);
        canvas.text(inside.x + 1, inside.y, "Files", Token::Fg, inside);
        canvas.text_right(inside.x + 19, inside.y, &grouped(files), Token::B, inside);
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
    let lines: u32 = counted
        .iter()
        .map(|file| file.counted_lines(tests).code)
        .sum();
    let rows = [
        ("Packages", count(&[SymbolKind::Package])),
        ("Modules", count(&[SymbolKind::Module])),
        ("Types", types),
        (
            "Functions",
            count(&[SymbolKind::Function, SymbolKind::Method]),
        ),
        ("Files", counted.len()),
        ("Lines", usize::try_from(lines).unwrap_or_default()),
    ];
    for (index, (label, value)) in rows.iter().enumerate() {
        let y = inside.y + u16::try_from(index).unwrap_or_default();
        canvas.text(inside.x + 1, y, label, Token::Fg, inside);
        canvas.text_right(inside.x + 19, y, &grouped(*value), Token::B, inside);
    }
    let note = if app.lens().tests {
        "tests counted"
    } else {
        "tests left out"
    };
    canvas.text(inside.x + 21, inside.y + 5, note, Token::Dim, inside);
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
        canvas.text(inside.x + 1, y, language, Token::Fg, inside);
        let x = canvas.text(inside.x + 12, y, &filled, Token::Acc, inside);
        let x = canvas.text(x, y, &rest, Token::Track, inside);
        let percent = steps(share, 100);
        canvas.text(x + 2, y, &format!("{percent}%"), Token::Fg, inside);
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
    let counts = format!("{errors} error   {warnings} warning");
    canvas.text(inside.x + 1, inside.y, &counts, Token::Fg, inside);
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
    let line = |index: u16| inside.y + index;
    if let Some(running) = app.running_analysis() {
        running_lines(app, running, canvas, inside);
    } else {
        let syntax = if app.stale() { "stale" } else { "fresh" };
        canvas.text(
            inside.x + 1,
            line(0),
            &format!("Syntax  {syntax}"),
            Token::Fg,
            inside,
        );
        let precise = if app.precise_layer() {
            "indexed"
        } else {
            "no index"
        };
        canvas.text(
            inside.x + 1,
            line(1),
            &format!("Precise {precise}"),
            Token::Fg,
            inside,
        );
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
            canvas.text(
                inside.x + 1,
                line(3),
                &format!("Exact   {share}"),
                Token::Fg,
                inside,
            );
            let problems = format!("Problems {}", analysis.problems().len());
            canvas.text(inside.x + 1, line(4), &problems, Token::Fg, inside);
            let tests = if app.lens().tests {
                "counted"
            } else {
                "left out"
            };
            canvas.text(
                inside.x + 1,
                line(5),
                &format!("Tests   {tests}"),
                Token::Fg,
                inside,
            );
            if let Some(problem) = analysis.problems().first() {
                canvas.text(
                    inside.x + 1,
                    line(7),
                    &problem.to_string(),
                    Token::Dim,
                    inside,
                );
            }
        }
        if let Some(failure) = app.failure() {
            canvas.text(inside.x + 1, line(7), failure, Token::H2, inside);
        }
        canvas.text(inside.x + 1, line(9), "Space s details", Token::Dim, inside);
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
    canvas.text(inside.x + 1, inside.y, "metric", Token::Faint, inside);
    canvas.text(inside.x + 15, inside.y, "spread", Token::Faint, inside);
    canvas.text(inside.x + 25, inside.y, "highest", Token::Faint, inside);
    canvas.text_right(
        inside.x + inside.width - 1,
        inside.y,
        "value",
        Token::Faint,
        inside,
    );
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
        if let Some(reason) = analysis.why_unavailable(*metric) {
            canvas.text(inside.x + 25, y, reason, Token::Dim, line);
        } else {
            let scope = metric.descriptor().measured_on();
            if let Some(spread) = analysis.spread(scope, *metric) {
                canvas.text(inside.x + 15, y, &sparkline(&spread), Token::Dim, line);
            }
            match analysis.worst(*metric) {
                Some(worst) => {
                    let names: Vec<String> =
                        worst.subjects.iter().take(3).map(short_subject).collect();
                    let value = metric.descriptor().format(worst.value);
                    let room = Rect {
                        width: inside.width.saturating_sub(34),
                        ..line
                    };
                    canvas.text(inside.x + 25, y, &names.join(", "), Token::Fg, room);
                    canvas.text_right(inside.x + inside.width - 1, y, &value, Token::Fg, line);
                }
                None => {
                    canvas.text_right(inside.x + inside.width - 1, y, "n/a", Token::Dim, line);
                }
            }
        }
        if *metric == app.lens().metric {
            canvas.paint(line, Token::Sel);
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
            let mut ranked: Vec<(&str, f64)> = partial
                .functions
                .iter()
                .filter_map(|function| {
                    let value = function.values.iter().find(|(one, _)| *one == metric)?.1;
                    Some((function.name.as_str(), value))
                })
                .collect();
            ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(b.0)));
            for (index, (name, value)) in ranked.iter().take(8).enumerate() {
                let y = inside.y + u16::try_from(index).unwrap_or_default();
                canvas.text(inside.x + 1, y, name, Token::Fg, inside);
                let shown = metric.descriptor().format(*value);
                canvas.text_right(inside.x + inside.width - 1, y, &shown, Token::Fg, inside);
            }
            let note = format!("From {} files. This list will change.", partial.files);
            canvas.text(inside.x + 1, inside.y + 9, &note, Token::Dim, inside);
        }
        return;
    };
    for (index, subject) in app.ranking().iter().enumerate() {
        let y = inside.y + u16::try_from(index).unwrap_or_default();
        let line = Rect {
            y,
            height: 1,
            ..inside
        };
        canvas.text(inside.x + 1, y, &subject.to_string(), Token::Fg, line);
        let value = analysis.value(subject, metric).map_or_else(
            || "n/a".to_owned(),
            |value| metric.descriptor().format(value),
        );
        canvas.text_right(inside.x + inside.width - 1, y, &value, Token::Fg, line);
        if active && index == app.ranking_row() {
            canvas.paint(line, Token::Sel);
        }
        if let Subject::Symbol(id) = subject {
            canvas.hit(line, Target::Symbol(id.clone()));
        }
    }
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
        (_, kind) if drawing.depth == 0 && kind != SymbolKind::Package => {
            format!("Inside {}", focus_name.0)
        }
        _ => format!("Neighbors of {}", focus_name.0),
    };
    let inside = canvas.pane(area, &title, active);
    let level = match drawing.level {
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
    let legend = inside.y + inside.height.saturating_sub(3);
    let rows = [
        "────── exact  ╌╌╌╌╌╌ inferred  ┄┄┄┄┄┄ ambiguous  ━━━━━━ cycle",
        "> uses, imports, calls  ▷ inherits  ○ implements  ◆ composes",
        "+ public  # restricted  - private   ! alert  @ module in a cycle",
    ];
    for (offset, row) in rows.iter().enumerate() {
        let y = legend + u16::try_from(offset).unwrap_or(0);
        canvas.text(inside.x + 1, y, row, Token::Dim, inside);
    }
}

fn map(app: &App, canvas: &mut Canvas<'_>, area: Rect, active: bool) {
    let title = app.focus().and_then(|focus| app.symbol(focus)).map_or_else(
        || "Map".to_owned(),
        |symbol| format!("Map of {}", symbol.name),
    );
    let inside = canvas.pane(area, &title, active);
    let Some(analysis) = app.analysis() else {
        return;
    };
    let tiles = app.tiles();
    let metric = app.lens().metric;
    let sizes: Vec<f64> = tiles.iter().map(|tile| app.tile_lines(&tile.id)).collect();
    let room = Rect {
        height: inside.height.saturating_sub(1),
        ..inside
    };
    let selected = app.selected();
    let marks = app.tree_marks();
    for (tile, slot) in tiles.iter().zip(squarify(&sizes, room)) {
        let tile_area = Rect {
            width: slot.width.saturating_sub(1).max(1),
            height: slot.height.saturating_sub(1).max(1),
            ..slot
        };
        let token = match value_token(analysis, tile, metric) {
            Token::H1 => Token::T2,
            Token::H2 => Token::T3,
            Token::H3 => Token::T4,
            Token::H0 => Token::T1,
            _ => Token::T0,
        };
        canvas.fill(tile_area, token);
        let marker = if selected.as_ref() == Some(&tile.id) {
            "▸"
        } else {
            ""
        };
        let mark = marks
            .get(&tile.id)
            .map_or_else(String::new, |mark| format!(" {mark}"));
        let label = format!("{marker}{}{mark}", tile.name);
        canvas.text(tile_area.x, tile_area.y, &label, token, tile_area);
        let value = app.shown_value(&tile.id, metric);
        canvas.text(tile_area.x, tile_area.y + 1, &value, token, tile_area);
        canvas.hit(tile_area, Target::Symbol(tile.id.clone()));
    }
    let mut legend = format!("area: lines   color: {}", app.metric_label());
    if app.lens().filter.is_some() {
        legend = format!(
            "{legend}   {} of {} tiles",
            tiles.len(),
            app.unfiltered_tiles()
        );
    }
    let y = inside.y + inside.height.saturating_sub(1);
    canvas.text(inside.x + 1, y, &legend, Token::Dim, inside);
}

fn find(app: &App, canvas: &mut Canvas<'_>, area: Rect, active: bool) {
    let inside = canvas.pane(area, "Find", active);
    let found = app.found();
    let total = app
        .analysis()
        .map_or(0, |analysis| analysis.view().symbols().count());
    let filter = app
        .lens()
        .filter
        .as_ref()
        .map_or("none", |filter| filter.text.as_str());
    canvas.text(
        inside.x + 1,
        inside.y,
        &format!("filter {filter}"),
        Token::Dim,
        inside,
    );
    let (sort, lowest_first) = app.sorted_by();
    let order = if lowest_first {
        "lowest first"
    } else {
        "highest first"
    };
    let summary = format!(
        "{} of {total}   sorted by {} {order}",
        found.len(),
        metric_label(sort)
    );
    canvas.text_right(
        inside.x + inside.width - 1,
        inside.y,
        &summary,
        Token::Dim,
        inside,
    );
    let mut columns: Vec<MetricId> =
        vec![MetricId::Cyclomatic, MetricId::Cognitive, MetricId::Lines];
    if !columns.contains(&sort) {
        columns.push(sort);
    }
    let width: u16 = 13;
    let first_column = (inside.x + inside.width)
        .saturating_sub(width * u16::try_from(columns.len()).unwrap_or(1) + 1);
    let heading_y = inside.y + 2;
    find_headings(canvas, inside, heading_y, first_column, &columns, sort);
    let selected = app.selected();
    let marks = app.tree_marks();
    for (index, symbol) in found.iter().enumerate() {
        let y = heading_y + 1 + u16::try_from(index).unwrap_or(u16::MAX);
        if y >= inside.y + inside.height {
            break;
        }
        let line = Rect {
            y,
            height: 1,
            ..inside
        };
        if let Some(mark) = marks.get(&symbol.id).filter(|mark| mark.starts_with('!')) {
            canvas.text(inside.x + 1, y, mark, Token::H2, line);
        }
        let name_room = Rect {
            width: first_column.saturating_sub(inside.x + 12),
            ..line
        };
        canvas.text(inside.x + 4, y, symbol.id.as_str(), Token::Fg, name_room);
        canvas.text(
            first_column.saturating_sub(10),
            y,
            symbol.kind.name(),
            Token::Dim,
            line,
        );
        for (column, metric) in columns.iter().enumerate() {
            let right = first_column + width * (u16::try_from(column).unwrap_or_default() + 1);
            let value = app.shown_value(&symbol.id, *metric);
            canvas.text_right(right, y, &value, Token::Fg, line);
        }
        canvas.hit(line, Target::Symbol(symbol.id.clone()));
        if selected.as_ref() == Some(&symbol.id) {
            canvas.paint(line, Token::Sel);
        }
    }
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
    let rules_area = Rect {
        height: (body.height / 2).max(8).min(body.height),
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
    for (row, hit) in rows.iter().enumerate() {
        let y = hits.y + u16::try_from(row).unwrap_or(u16::MAX);
        let line = Rect {
            y,
            height: 1,
            ..hits
        };
        canvas.text(hits.x + 1, y, hit, Token::Fg, line);
        if active == Some(Pane::Hits) && row == app.selected_hit() {
            canvas.paint(line, Token::Sel);
        }
    }
    alerts_ci(app, analysis, canvas, ci_area);
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
    let half = body.height / 2;
    let pipeline = Rect {
        height: half,
        ..left
    };
    let problems = Rect {
        y: left.y + half,
        height: left.height - half,
        ..left
    };
    let precise = Rect {
        height: half,
        ..right
    };
    let lower = Rect {
        y: right.y + half,
        height: right.height - half,
        ..right
    };
    status_pipeline(app, canvas, pipeline);
    status_problems(app, canvas, problems);
    status_precise(app, canvas, precise);
    status_left_out(app, canvas, lower);
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
        Some(Popup::ConfirmPrecise) => confirm_precise(canvas, body),
        Some(Popup::SortPicker { typed }) => picker(canvas, body, "Sort by", typed),
        Some(Popup::RelationPicker {
            typed,
            ticked,
            cursor,
        }) => relation_picker(canvas, body, typed, ticked, *cursor),
        Some(Popup::MoreNodes { hidden, cursor }) => more_nodes(app, canvas, body, hidden, *cursor),
        Some(Popup::ContextMenu { cursor }) => context_menu(app, canvas, body, *cursor),
        None => {}
    }
}

fn popup_frame(canvas: &mut Canvas<'_>, area: Rect, title: &str) {
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

fn confirm_precise(canvas: &mut Canvas<'_>, body: Rect) {
    let area = Rect {
        x: body.x + body.width.saturating_sub(64) / 2,
        y: body.y + body.height / 3,
        width: 64.min(body.width),
        height: 7.min(body.height),
    };
    popup_frame(canvas, area, "");
    canvas.text(
        area.x + 2,
        area.y + 1,
        "Run precise analysis?",
        Token::Popb,
        area,
    );
    canvas.text(
        area.x + 2,
        area.y + 3,
        "Indexing is slow, and may run the project's build scripts.",
        Token::Pop,
        area,
    );
    let x = canvas.text(area.x + 2, area.y + 5, "y", Token::Popk, area);
    let x = canvas.text(x + 1, area.y + 5, "run it", Token::Pop, area);
    let x = canvas.text(x + 3, area.y + 5, "n", Token::Popk, area);
    canvas.text(x + 1, area.y + 5, "not now", Token::Pop, area);
}

fn metric_picker(canvas: &mut Canvas<'_>, body: Rect, typed: &str) {
    picker(canvas, body, "Metric", typed);
}

/// A list of metrics that narrows as you type.
fn picker(canvas: &mut Canvas<'_>, body: Rect, title: &str, typed: &str) {
    let choices = picked(typed);
    let height = u16::try_from(choices.len()).unwrap_or_default() + 4;
    let area = Rect {
        x: body.x + (body.width.saturating_sub(60)) / 2,
        y: body.y + 1,
        width: 60.min(body.width),
        height: height.min(body.height),
    };
    popup_frame(canvas, area, title);
    canvas.text(
        area.x + 2,
        area.y + 1,
        &format!("› {typed}█"),
        Token::Popb,
        area,
    );
    for (index, metric) in choices.iter().enumerate() {
        let y = area.y + 3 + u16::try_from(index).unwrap_or_default();
        let token = if index == 0 { Token::Pops } else { Token::Pop };
        canvas.text(area.x + 2, y, metric_label(*metric), token, area);
        canvas.text(area.x + 18, y, metric.measures(), Token::Popd, area);
        if metric.descriptor().heat.is_none() {
            canvas.text_right(area.x + area.width - 2, y, "neutral", Token::Popd, area);
        }
    }
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
    for (index, (key, meaning)) in entries.iter().enumerate() {
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

/// The one-eyed watcher, in its pose. The art comes with the rest of the design.
fn mascot(app: &App, canvas: &mut Canvas<'_>, inside: Rect) {
    let eye = match app.pose() {
        Pose::Watching => "( ◉ )",
        Pose::Scanning => "(◉  )",
        Pose::Found => "( ◉ )!",
        Pose::NothingHere => "( - )",
    };
    let token = if app.pose() == Pose::Found {
        Token::H2
    } else {
        Token::Acc
    };
    canvas.text(
        inside.x + inside.width.saturating_sub(10),
        inside.y + 2,
        eye,
        token,
        inside,
    );
}

/// Eight ranges as one cell each, from `▁` to `▇`.
fn sparkline(spread: &[usize; 8]) -> String {
    let glyphs = ['▁', '▂', '▃', '▄', '▅', '▆', '▇'];
    let highest = spread.iter().copied().max().unwrap_or(1).max(1);
    spread
        .iter()
        .map(|count| {
            if *count == 0 {
                ' '
            } else {
                let level = (count * 6).div_ceil(highest);
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
            let shown = metric.descriptor().format(value);
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

/// The headings of Find's columns, each of which sorts by its metric when clicked.
fn find_headings(
    canvas: &mut Canvas<'_>,
    inside: Rect,
    heading_y: u16,
    first_column: u16,
    columns: &[MetricId],
    sort: MetricId,
) {
    let width: u16 = 13;
    canvas.text(inside.x + 1, heading_y, "symbol", Token::Faint, inside);
    canvas.text(
        first_column.saturating_sub(10),
        heading_y,
        "kind",
        Token::Faint,
        inside,
    );
    for (index, metric) in columns.iter().enumerate() {
        let right = first_column + width * (u16::try_from(index).unwrap_or_default() + 1);
        let label = metric_label(*metric);
        let token = if *metric == sort {
            Token::Accb
        } else {
            Token::Faint
        };
        canvas.text_right(right, heading_y, label, token, inside);
        let start = right.saturating_sub(width);
        canvas.hit(
            Rect {
                x: start,
                y: heading_y,
                width,
                height: 1,
            },
            Target::Column(*metric),
        );
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
    let room = u32::from(inside.height);
    let first = current.saturating_sub(room / 3).max(1);
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
        canvas.text(inside.x + 1, y, &format!("{number:>5}"), Token::Faint, row);
        if let Some(added) = points.get(&number) {
            canvas.text(inside.x + 8, y, &format!("+{added}"), Token::H1, row);
        }
        canvas.text(inside.x + 13, y, line, Token::Fg, row);
        if number == current {
            canvas.paint(row, Token::Sel);
        }
    }
    inspector(app, canvas, right, app.pane() == Some(Pane::Inspector));
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
            let value = alert.metric.descriptor().format(alert.value);
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
            let value = rule.metric.descriptor().format(value);
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
    let inside = canvas.pane(area, "Last analysis", app.pane() == Some(Pane::Status));
    if let Some(failure) = app.failure() {
        canvas.text(inside.x + 1, inside.y, failure, Token::H2, inside);
    }
    let Some(analysis) = app.analysis() else {
        return;
    };
    canvas.text(
        inside.x + 1,
        inside.y,
        "step         time result",
        Token::Faint,
        inside,
    );
    for (row, report) in analysis.steps().iter().enumerate() {
        let y = inside.y + 1 + u16::try_from(row).unwrap_or(u16::MAX);
        let seconds = format!("{}.{}s", report.millis / 1000, (report.millis % 1000) / 100);
        canvas.text(
            inside.x + 1,
            y,
            &capitalized(report.step.name()),
            Token::Fg,
            inside,
        );
        canvas.text_right(inside.x + 18, y, &seconds, Token::Dim, inside);
        canvas.text(inside.x + 19, y, &report.result, Token::Fg, inside);
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
    let title = format!("Problems: {}", problems.len());
    let inside = canvas.pane(area, &title, false);
    if problems.is_empty() {
        canvas.text(
            inside.x + 1,
            inside.y,
            "Every file was read.",
            Token::Dim,
            inside,
        );
    }
    for (row, problem) in problems.iter().enumerate() {
        let y = inside.y + u16::try_from(row).unwrap_or(u16::MAX);
        let line = Rect {
            y,
            height: 1,
            ..inside
        };
        canvas.text(inside.x + 1, y, problem, Token::Fg, line);
        if app.problem_at() == Some(row) {
            canvas.paint(line, Token::Sel);
        }
    }
}

fn status_precise(app: &App, canvas: &mut Canvas<'_>, area: Rect) {
    let inside = canvas.pane(area, "Precise analysis", false);
    canvas.text(
        inside.x + 1,
        inside.y,
        "language                 indexer          ready?",
        Token::Faint,
        inside,
    );
    let mut y = inside.y + 1;
    for report in app.indexers() {
        let ready = if report.ready.is_ok() { "yes" } else { "no" };
        let languages: Vec<&str> = report
            .languages
            .iter()
            .map(|language| language.name())
            .collect();
        canvas.text(inside.x + 1, y, &languages.join(", "), Token::Fg, inside);
        canvas.text(inside.x + 26, y, &report.indexer, Token::Fg, inside);
        canvas.text(inside.x + 43, y, ready, Token::Fg, inside);
        y += 1;
        if let Err(reason) = &report.ready {
            canvas.text(inside.x + 3, y, reason, Token::Dim, inside);
            y += 1;
        }
    }
    y += 1;
    if let Some(running) = app.running_analysis().filter(|running| running.precise) {
        let elapsed = app.now().saturating_sub(running.since) / 1000;
        let indexer = running.indexer.as_deref().unwrap_or("the indexer");
        let line = format!("Running {indexer} for {elapsed}s. No progress reported.");
        canvas.text(inside.x + 1, y, &line, Token::Fg, inside);
        y += 1;
    }
    let index = app.analysis().map_or_else(
        || "none yet".to_owned(),
        |analysis| {
            let status = analysis.precise();
            if status.current.is_none() {
                "none yet".to_owned()
            } else {
                plural(status.changed, "file") + " changed"
            }
        },
    );
    canvas.text(
        inside.x + 1,
        y,
        &format!("index {index}"),
        Token::Fg,
        inside,
    );
}

fn status_left_out(app: &App, canvas: &mut Canvas<'_>, area: Rect) {
    let inside = canvas.pane(area, "What is left out", false);
    let Some(analysis) = app.analysis() else {
        return;
    };
    let test_files: Vec<_> = analysis
        .files()
        .iter()
        .filter(|file| file.role == CodeRole::Test || file.test_lines.code > 0)
        .collect();
    let lines: u32 = test_files.iter().map(|file| file.test_lines.code).sum();
    let state = if app.lens().tests {
        "Included"
    } else {
        "Left out"
    };
    let test_code = format!(
        "Test code  {state}: {}, {}",
        plural(test_files.len(), "file"),
        plural(usize::try_from(lines).unwrap_or(0), "line")
    );
    canvas.text(inside.x + 1, inside.y, &test_code, Token::Fg, inside);
    let unavailable: Vec<String> = analysis
        .unavailable()
        .iter()
        .map(ToString::to_string)
        .collect();
    if unavailable.is_empty() {
        canvas.text(
            inside.x + 1,
            inside.y + 2,
            "Metrics unavailable: none",
            Token::Fg,
            inside,
        );
    } else {
        canvas.text(
            inside.x + 1,
            inside.y + 2,
            "Metrics unavailable:",
            Token::Fg,
            inside,
        );
        for (row, line) in unavailable.iter().enumerate() {
            let y = inside.y + 3 + u16::try_from(row).unwrap_or(u16::MAX);
            canvas.text(inside.x + 3, y, line, Token::Fg, inside);
        }
    }
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
                    format!("   uses {}", group.fields.join(", ")),
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
        | MetricId::Distance => {
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
            vec![
                format!("Depends on {}", out.len()),
                format!("  {}", out.join(", ")),
                format!("Depended on by {}", into.len()),
                format!("  {}", into.join(", ")),
            ]
        }
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
    }
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
    let name = app.symbol(&card.id).map_or_else(
        || card.id.to_string(),
        |symbol| {
            if symbol.kind.is_type() {
                symbol.name.clone()
            } else {
                symbol.id.to_string()
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
    canvas.text(inside.x + 1, y, &label, token, name_row);
    let value = app.shown_value(&card.id, metric);
    canvas.text_right(inside.x + inside.width, y, &value, Token::Fg, name_row);
    if is_selected {
        canvas.paint(name_row, Token::Sel);
    }
    canvas.hit(name_row, Target::Symbol(card.id.clone()));
    y += 1;
    draw_box_rows(canvas, card, rows, y, selected, metric, (border, divider));
}

/// Splits an area into one rectangle for each weight, keeping each close to square: a
/// squarified treemap. A cell is about twice as tall as it is wide, so widths count
/// double.
fn squarify(weights: &[f64], area: Rect) -> Vec<Rect> {
    let weight = |index: usize| weights.get(index).copied().unwrap_or(0.0);
    let mut placed = vec![Rect::default(); weights.len()];
    let mut order: Vec<usize> = (0..weights.len()).collect();
    order.sort_by(|a, b| weight(*b).total_cmp(&weight(*a)));
    let mut left = area;
    let mut remaining: f64 = weights.iter().sum();
    let mut index = 0;
    while index < order.len() && left.width > 0 && left.height > 0 && remaining > 0.0 {
        let wide = left.width >= left.height * 2;
        let side = if wide {
            f64::from(left.height) * 2.0
        } else {
            f64::from(left.width)
        };
        let scale = f64::from(left.width) * f64::from(left.height) * 2.0 / remaining;
        let row = best_row(order.get(index..).unwrap_or_default(), &weight, scale, side);
        let row_weight: f64 = row.iter().map(|member| weight(*member)).sum();
        let last = index + row.len() >= order.len();
        left = place_row(
            &mut placed,
            &row,
            &weight,
            left,
            row_weight / remaining,
            wide,
            last,
        );
        remaining -= row_weight;
        index += row.len().max(1);
    }
    placed
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
    let area = Rect {
        x: body.x + body.width.saturating_sub(60) / 2,
        y: body.y + 1,
        width: 60.min(body.width),
        height: (u16::try_from(choices.len()).unwrap_or(0) + 4).min(body.height),
    };
    popup_frame(canvas, area, "Relations");
    canvas.text(
        area.x + 2,
        area.y + 1,
        &format!("› {typed}█"),
        Token::Popb,
        area,
    );
    let entries: Vec<(String, String)> = choices
        .iter()
        .map(|kind| {
            let tick = if ticked.contains(kind) { "[x]" } else { "[ ]" };
            (format!("{tick} {}", kind.name()), String::new())
        })
        .collect();
    popup_entries(canvas, area, &entries, cursor);
}

fn more_nodes(app: &App, canvas: &mut Canvas<'_>, body: Rect, hidden: &[SymbolId], cursor: usize) {
    let area = Rect {
        x: body.x + body.width.saturating_sub(56) / 2,
        y: body.y + 1,
        width: 56.min(body.width),
        height: (u16::try_from(hidden.len())
            .unwrap_or(u16::MAX)
            .saturating_add(3))
        .min(body.height),
    };
    popup_frame(canvas, area, &format!("{} more", hidden.len()));
    let metric = app.lens().metric;
    let entries: Vec<(String, String)> = hidden
        .iter()
        .map(|id| (id.to_string(), app.shown_value(id, metric)))
        .collect();
    popup_entries(
        canvas,
        Rect {
            y: area.y.saturating_sub(1),
            ..area
        },
        &entries,
        cursor,
    );
}

fn context_menu(app: &App, canvas: &mut Canvas<'_>, body: Rect, cursor: usize) {
    let name = app
        .selected()
        .and_then(|id| app.symbol(&id).map(|symbol| symbol.name.clone()))
        .unwrap_or_default();
    let area = Rect {
        x: body.x + 20.min(body.width / 4),
        y: body.y + 3,
        width: 38.min(body.width),
        height: (u16::try_from(CONTEXT_MENU.len()).unwrap_or(0) + 3).min(body.height),
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
                canvas.text(
                    inside.x,
                    y,
                    &format!("{pointer}{text}{mark}"),
                    Token::Fg,
                    line,
                );
                let shown = value.map_or_else(
                    || "n/a".to_owned(),
                    |value| metric.descriptor().format(value),
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

/// The members of the next row: as many as keep the worst ratio of sides from growing.
fn best_row(next: &[usize], weight: &dyn Fn(usize) -> f64, scale: f64, side: f64) -> Vec<usize> {
    let mut row: Vec<usize> = Vec::new();
    let mut total = 0.0;
    let mut worst = f64::INFINITY;
    for member in next {
        let trial = total + weight(*member);
        let thickness = trial * scale / side;
        let ratio = row
            .iter()
            .chain(std::iter::once(member))
            .map(|one| {
                let length = weight(*one) * scale / thickness;
                (length / thickness).max(thickness / length)
            })
            .fold(0.0, f64::max);
        if !row.is_empty() && ratio > worst {
            break;
        }
        worst = ratio;
        row.push(*member);
        total = trial;
    }
    row
}

/// Places one row along the short side of what is left, and returns what is left after.
fn place_row(
    placed: &mut [Rect],
    row: &[usize],
    weight: &dyn Fn(usize) -> f64,
    left: Rect,
    share: f64,
    wide: bool,
    last: bool,
) -> Rect {
    let total: f64 = row.iter().map(|member| weight(*member)).sum();
    let (across, along) = if wide {
        (left.width, left.height)
    } else {
        (left.height, left.width)
    };
    let thickness = if last {
        across
    } else {
        steps(share, across).max(1)
    };
    let mut offset = 0;
    for (position, member) in row.iter().enumerate() {
        let length = if position + 1 == row.len() {
            along - offset
        } else {
            steps(weight(*member) / total.max(f64::MIN_POSITIVE), along).max(1)
        };
        let rect = if wide {
            Rect::new(left.x, left.y + offset, thickness, length)
        } else {
            Rect::new(left.x + offset, left.y, length, thickness)
        };
        if let Some(slot) = placed.get_mut(*member) {
            *slot = rect;
        }
        offset += length;
    }
    if wide {
        Rect::new(
            left.x + thickness,
            left.y,
            left.width.saturating_sub(thickness),
            left.height,
        )
    } else {
        Rect::new(
            left.x,
            left.y + thickness,
            left.width,
            left.height.saturating_sub(thickness),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cells(rect: Rect) -> u32 {
        u32::from(rect.width) * u32::from(rect.height)
    }

    #[test]
    fn a_treemap_fills_the_area_and_larger_weighs_more() {
        let area = Rect::new(0, 0, 60, 20);
        let tiles = squarify(&[10.0, 40.0, 20.0, 30.0], area);
        let total: u32 = tiles.iter().map(|tile| cells(*tile)).sum();
        assert_eq!(total, cells(area));
        let size = |index: usize| tiles.get(index).map_or(0, |tile| cells(*tile));
        assert!(size(1) > size(3) && size(3) > size(2) && size(2) > size(0));
        for tile in &tiles {
            assert!(
                tile.x + tile.width <= 60 && tile.y + tile.height <= 20,
                "{tile:?}"
            );
        }
    }

    #[test]
    fn nothing_to_lay_out_is_nothing_drawn() {
        assert_eq!(squarify(&[], Rect::new(0, 0, 10, 10)), Vec::new());
    }
}

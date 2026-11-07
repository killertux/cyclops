//! What the interface looks like: the shell, each view, the inspector and the popups.
//! See `specs/tui/design/`.

use crate::app::{App, MIN_COLUMNS, MIN_ROWS, Running, SIDE_BY_SIDE, hints, picked, space_menu};
use crate::canvas::{Canvas, fit, width};
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
use cyclops_domain::settings::Severity;
use cyclops_domain::symbol::CodeRole;
use cyclops_domain::symbol::{Symbol, SymbolKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

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
    match app.view() {
        View::Overview => overview(app, canvas, body),
        View::Explore => with_inspector(app, canvas, body, 58, structure),
        View::Diagram => with_inspector(app, canvas, body, 36, diagram),
        View::Map => with_inspector(app, canvas, body, 36, map),
        View::Find => with_inspector(app, canvas, body, 40, find),
        View::Alerts => alerts(app, canvas, body),
        View::Status => status(app, canvas, body),
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
    let mut x = area.x + 1;
    let last = parts.len().saturating_sub(1);
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            x = canvas.text(x, y, " › ", Token::Faint, area);
        }
        let token = if index == last { Token::B } else { Token::Fg };
        x = canvas.text(x, y, part, token, area);
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
        x = canvas.text(x + 2, y, label, Token::Chip, line);
        x = canvas.text(x + 1, y, value, Token::Chipa, line);
    }
    let mut parts: Vec<(String, Token)> = Vec::new();
    if let Some(analysis) = app.analysis() {
        let alerts = analysis.alerts().len();
        parts.push((plural(alerts, "alert"), Token::Chiph));
        parts.push((plural(analysis.problems().len(), "problem"), Token::Chip));
    } else if app.failure().is_some() {
        parts.push((plural(1, "problem"), Token::Chip));
    }
    let freshness = if app.analyzing() {
        ("analyzing".to_owned(), Token::Chipa)
    } else if app.stale() {
        ("stale".to_owned(), Token::Chipa)
    } else {
        ("fresh".to_owned(), Token::Chipd)
    };
    parts.push(freshness);
    let total: u16 = parts.iter().map(|(text, _)| width(text) + 2).sum();
    let mut x = (area.x + area.width).saturating_sub(total).max(x + 2);
    for (text, token) in parts {
        x = canvas.text(x, y, &text, token, line) + 2;
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
    if let Some(message) = app.message() {
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
    let side = inspector_width.min(body.width / 2);
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
    canvas.text(
        body.x + 1,
        body.y,
        &format!("Why {} is {value}", metric_label(metric)),
        Token::B,
        body,
    );
    if let Some(reason) = analysis.why_unavailable(metric) {
        canvas.text(body.x + 1, body.y + 2, reason, Token::Dim, body);
        return;
    }
    let Some(explanation) = analysis.explain(&symbol.id) else {
        canvas.text(
            body.x + 1,
            body.y + 2,
            "Nothing to explain here.",
            Token::Dim,
            body,
        );
        return;
    };
    let mut y = body.y + 2;
    canvas.text(
        body.x + 1,
        y,
        &format!("Decision points {}", explanation.decisions),
        Token::Fg,
        body,
    );
    y += 1;
    canvas.text(
        body.x + 1,
        y,
        &format!("Added by nesting {}", explanation.nesting),
        Token::Fg,
        body,
    );
    y += 2;
    for (line, points) in &explanation.points {
        canvas.text(
            body.x + 1,
            y,
            &format!("line {line}  +{points}"),
            Token::Dim,
            body,
        );
        y += 1;
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
    canvas.text(
        body.x + 1,
        body.y,
        &format!("Reach {}", impact.all().count()),
        Token::B,
        body,
    );
    let mut y = body.y + 2;
    for (distance, level) in impact.levels.iter().enumerate() {
        for (id, confidence) in level {
            let guess = if *confidence == cyclops_domain::relation::Confidence::Exact {
                ""
            } else {
                " ~"
            };
            canvas.text(
                body.x + 1,
                y,
                &format!("{}  {id}{guess}", distance + 1),
                Token::Fg,
                body,
            );
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
    let Some(drawing) = app.drawing() else {
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
    let controls = format!(
        "level {level}   depth {}   overlay {}   relations all",
        drawing.depth,
        app.metric_label()
    );
    canvas.text(inside.x + 1, inside.y, &controls, Token::Dim, inside);
    let columns = drawing
        .placed
        .iter()
        .map(|placed| placed.column)
        .max()
        .map_or(1, |most| most + 1);
    let columns = u16::try_from(columns).unwrap_or(1).max(1);
    let column_width = (inside.width / columns).clamp(12, 28);
    let selected = app.selected();
    let metric = app.lens().metric;
    for placed in &drawing.placed {
        let x = inside.x + 1 + u16::try_from(placed.column).unwrap_or_default() * column_width;
        let y = inside.y + 2 + u16::try_from(placed.row).unwrap_or_default() * 4;
        let node_area = Rect {
            x,
            y,
            width: column_width.saturating_sub(4),
            height: 3,
        };
        if node_area.y + node_area.height > inside.y + inside.height
            || node_area.x + node_area.width > inside.x + inside.width
        {
            continue;
        }
        let focused = Some(&placed.id) == app.focus();
        if focused {
            canvas.frame(node_area, Token::Acc, ['┏', '┓', '┗', '┛', '━', '┃']);
        } else {
            canvas.frame(node_area, Token::Line, ['╭', '╮', '╰', '╯', '─', '│']);
        }
        let name = app
            .symbol(&placed.id)
            .map(|symbol| symbol.name.clone())
            .unwrap_or_default();
        let is_selected = selected.as_ref() == Some(&placed.id);
        let label = if is_selected {
            format!("▸{name}")
        } else {
            name
        };
        let name_row = Rect {
            x: node_area.x + 1,
            y: y + 1,
            width: node_area.width.saturating_sub(2),
            height: 1,
        };
        let token = if focused { Token::Accb } else { Token::B };
        canvas.text(name_row.x, name_row.y, &label, token, name_row);
        let value = app.shown_value(&placed.id, metric);
        canvas.text_right(
            name_row.x + name_row.width,
            name_row.y,
            &value,
            Token::Fg,
            name_row,
        );
        if is_selected {
            canvas.paint(name_row, Token::Sel);
        }
        canvas.hit(node_area, Target::Symbol(placed.id.clone()));
    }
    let legend_y = inside.y + inside.height.saturating_sub(1);
    canvas.text(
        inside.x + 1,
        legend_y,
        "─ exact   ╌ inferred   ┄ ambiguous   > uses, calls, imports",
        Token::Dim,
        inside,
    );
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
    let sizes: Vec<f64> = tiles
        .iter()
        .map(|tile| {
            analysis
                .value(&Subject::Symbol(tile.id.clone()), MetricId::Lines)
                .unwrap_or(1.0)
                .max(1.0)
        })
        .collect();
    let total: f64 = sizes.iter().sum();
    let room = Rect {
        height: inside.height.saturating_sub(1),
        ..inside
    };
    let mut x = room.x;
    let selected = app.selected();
    for (tile, size) in tiles.iter().zip(&sizes) {
        let share = if total > 0.0 { size / total } else { 0.0 };
        let cells = steps(share, room.width).max(1);
        let tile_area = Rect {
            x,
            y: room.y,
            width: cells.saturating_sub(1).max(1),
            height: room.height,
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
        canvas.text(
            tile_area.x,
            tile_area.y,
            &format!("{marker}{}", tile.name),
            token,
            tile_area,
        );
        canvas.text(
            tile_area.x,
            tile_area.y + 1,
            &app.shown_value(&tile.id, metric),
            token,
            tile_area,
        );
        canvas.hit(tile_area, Target::Symbol(tile.id.clone()));
        x += cells;
    }
    let legend = format!("area: lines   color: {}", app.metric_label());
    canvas.text(
        inside.x + 1,
        inside.y + inside.height.saturating_sub(1),
        &legend,
        Token::Dim,
        inside,
    );
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
    let active = app.pane();
    let rules = canvas.pane(left, "Rules", active == Some(Pane::Rules));
    let hits = canvas.pane(right, "Hits", active == Some(Pane::Hits));
    let Some(analysis) = app.analysis() else {
        return;
    };
    let mut by_rule: Vec<(String, usize)> = Vec::new();
    for alert in analysis.alerts() {
        let name = alert
            .rule
            .clone()
            .unwrap_or_else(|| alert.metric.name().to_owned());
        match by_rule.iter_mut().find(|(rule, _)| *rule == name) {
            Some((_, count)) => *count += 1,
            None => by_rule.push((name, 1)),
        }
    }
    for (index, (rule, count)) in by_rule.iter().enumerate() {
        let y = rules.y + u16::try_from(index).unwrap_or(u16::MAX);
        canvas.text(
            rules.x + 1,
            y,
            &format!("{count:>3}  {rule}"),
            Token::Fg,
            rules,
        );
    }
    for (index, alert) in analysis.alerts().iter().enumerate() {
        let y = hits.y + u16::try_from(index).unwrap_or(u16::MAX);
        let mark = if alert.severity == Severity::Error {
            "!!"
        } else {
            "!"
        };
        let value = alert.metric.descriptor().format(alert.value);
        canvas.text(
            hits.x + 1,
            y,
            &format!("{mark:<3}{}  {}={value}", alert.subject, alert.metric),
            Token::Fg,
            hits,
        );
    }
}

fn status(app: &App, canvas: &mut Canvas<'_>, body: Rect) {
    let inside = canvas.pane(body, "Status", app.pane() == Some(Pane::Status));
    let mut y = inside.y;
    let layer = if app.precise_layer() {
        "precise"
    } else {
        "syntax"
    };
    canvas.text(
        inside.x + 1,
        y,
        &format!("Layer {layer}"),
        Token::Fg,
        inside,
    );
    y += 2;
    if let Some(failure) = app.failure() {
        canvas.text(inside.x + 1, y, failure, Token::H2, inside);
        y += 1;
    }
    if let Some(analysis) = app.analysis() {
        canvas.text(inside.x + 1, y, "Problems", Token::B, inside);
        y += 1;
        for problem in analysis.problems() {
            canvas.text(inside.x + 1, y, &problem.to_string(), Token::Fg, inside);
            y += 1;
        }
        for unavailable in analysis.unavailable() {
            canvas.text(
                inside.x + 1,
                y,
                &unavailable.to_string(),
                Token::Dim,
                inside,
            );
            y += 1;
        }
    }
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
        canvas.text(
            inside.x + 1,
            inside.y,
            "Precise running",
            Token::Accb,
            inside,
        );
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

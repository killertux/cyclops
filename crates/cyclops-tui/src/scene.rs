//! Laying out a diagram on a grid of cells: where each node and class box goes, which
//! nodes fold into a `+N more` card, and how each relation is routed. Both the keys and
//! the drawing use it, so what moves is what is shown.

use crate::app::Placed;
use cyclops_application::diagram::{DiagramView, Edge, Level, Member, Node};
use cyclops_domain::relation::Confidence;
use cyclops_domain::symbol::{SymbolId, Visibility};
use ratatui::layout::Rect;
use std::collections::{BTreeMap, HashSet};

/// The width of a node, at the design size.
pub const CARD_WIDTH: u16 = 24;
/// The columns between two columns of nodes, where relations run.
const GAP: u16 = 6;
/// How many rows of each compartment a class box shows: on the focus, and elsewhere.
const FOCUS_ROWS: usize = 5;
const NEIGHBOR_ROWS: usize = 2;

/// A row of a class box.
#[derive(Debug, Clone, PartialEq)]
pub enum BoxRow {
    Stereotype(&'static str),
    Divider,
    Member {
        id: SymbolId,
        text: String,
        value: Option<f64>,
        alerted: bool,
    },
    More(usize),
}

/// A node, where it is drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    pub id: SymbolId,
    pub column: usize,
    pub area: Rect,
    /// Rows below the name, for a class box.
    pub rows: Vec<BoxRow>,
}

impl Card {
    /// The row of the card a member is on, if it shows.
    #[must_use]
    pub fn row_of(&self, member: &SymbolId) -> Option<u16> {
        let index = self.rows.iter().position(|row| match row {
            BoxRow::Member { id, .. } => id == member,
            _ => false,
        })?;
        let index = u16::try_from(index).unwrap_or(0);
        // A stereotype sits above the name, and every other row below it, so a row is
        // always two below the top border plus its place.
        Some(self.area.y + 2 + index)
    }
}

/// The nodes of a column that did not fit, folded into one card.
#[derive(Debug, Clone, PartialEq)]
pub struct MoreCard {
    pub column: usize,
    pub area: Rect,
    pub hidden: Vec<SymbolId>,
}

/// A whole diagram, laid out.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scene {
    pub cards: Vec<Card>,
    pub more: Vec<MoreCard>,
}

impl Scene {
    #[must_use]
    pub fn card(&self, id: &SymbolId) -> Option<&Card> {
        self.cards.iter().find(|card| &card.id == id)
    }

    /// The card a symbol is on: its own, or its type's.
    #[must_use]
    pub fn card_holding(&self, id: &SymbolId) -> Option<&Card> {
        self.card(id).or_else(|| {
            self.cards.iter().find(|card| {
                card.rows
                    .iter()
                    .any(|row| matches!(row, BoxRow::Member { id: member, .. } if member == id))
            })
        })
    }
}

/// What the layout needs to know besides the diagram.
pub struct Setting<'a> {
    pub level: Level,
    pub focus: Option<&'a SymbolId>,
    /// Class boxes that show every member.
    pub expanded: &'a HashSet<SymbolId>,
    /// A node chosen from a `+N more` card, which takes the last place of its column.
    pub promoted: Option<&'a SymbolId>,
}

fn marker(visibility: Visibility) -> char {
    match visibility {
        Visibility::Public => '+',
        Visibility::Restricted => '#',
        Visibility::Private => '-',
    }
}

/// The members a class box lists: fields alphabetically, then methods worst first.
fn members(node: &Node, shown: usize) -> Vec<BoxRow> {
    let mut rows = Vec::new();
    if let Some(stereotype) = node.stereotype {
        rows.push(BoxRow::Stereotype(stereotype));
    }
    let mut methods: Vec<&Member> = node.methods.iter().collect();
    methods.sort_by(|a, b| match (a.overlay, b.overlay) {
        (Some(x), Some(y)) => y.total_cmp(&x).then_with(|| a.name.cmp(&b.name)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.name.cmp(&b.name),
    });
    for (compartment, method) in [
        (node.fields.iter().collect::<Vec<_>>(), false),
        (methods, true),
    ] {
        if compartment.is_empty() {
            continue;
        }
        rows.push(BoxRow::Divider);
        for member in compartment.iter().take(shown) {
            let call = if method { "()" } else { "" };
            rows.push(BoxRow::Member {
                id: node.id.child(&member.name),
                text: format!("{} {}{call}", marker(member.visibility), member.name),
                value: member.overlay,
                alerted: member.alerted,
            });
        }
        if compartment.len() > shown {
            rows.push(BoxRow::More(compartment.len() - shown));
        }
    }
    rows
}

/// How tall a card is: its border, its name, and its rows.
fn height(rows: &[BoxRow]) -> u16 {
    u16::try_from(rows.len())
        .unwrap_or(u16::MAX)
        .saturating_add(3)
}

/// Lays a diagram out in `area`.
#[must_use]
pub fn lay_out(view: &DiagramView, placed: &[Placed], setting: &Setting<'_>, area: Rect) -> Scene {
    let nodes: BTreeMap<&SymbolId, &Node> =
        view.nodes.iter().map(|node| (&node.id, node)).collect();
    let mut columns: BTreeMap<usize, Vec<&SymbolId>> = BTreeMap::new();
    for one in placed {
        columns.entry(one.column).or_default().push(&one.id);
    }
    let count = u16::try_from(columns.len()).unwrap_or(1).max(1);
    let width = CARD_WIDTH
        .min(area.width.saturating_sub(GAP * (count - 1)) / count)
        .max(10);
    let used = width * count + GAP * (count - 1);
    let left = area.x + area.width.saturating_sub(used) / 2;
    let bottom = area.y + area.height;
    let mut scene = Scene::default();
    for (index, (column, ids)) in columns.iter().enumerate() {
        let x = left + u16::try_from(index).unwrap_or(0) * (width + GAP);
        let mut ids = ids.clone();
        if let Some(promoted) = setting.promoted
            && let Some(at) = ids.iter().position(|id| *id == promoted)
        {
            let chosen = ids.remove(at);
            ids.insert(0, chosen);
        }
        let mut y = area.y;
        let mut hidden: Vec<SymbolId> = Vec::new();
        for (row, id) in ids.iter().enumerate() {
            let Some(node) = nodes.get(id) else {
                continue;
            };
            let rows = if setting.level == Level::Type {
                let shown = if setting.expanded.contains(*id) {
                    usize::MAX
                } else if setting.focus == Some(*id) {
                    FOCUS_ROWS
                } else {
                    NEIGHBOR_ROWS
                };
                members(node, shown)
            } else {
                Vec::new()
            };
            let card_height = height(&rows);
            let remaining = ids.len() - row - 1;
            let room_for_more = if remaining > 0 { 4 } else { 0 };
            if !hidden.is_empty() || y + card_height + room_for_more > bottom {
                hidden.push((*id).clone());
                continue;
            }
            scene.cards.push(Card {
                id: (*id).clone(),
                column: *column,
                area: Rect {
                    x,
                    y,
                    width,
                    height: card_height,
                },
                rows,
            });
            y += card_height + 1;
        }
        if !hidden.is_empty() {
            scene.more.push(MoreCard {
                column: *column,
                area: Rect {
                    x,
                    y: y.min(bottom.saturating_sub(3)),
                    width,
                    height: 3,
                },
                hidden,
            });
        }
    }
    scene
}

/// A relation to draw, from one cell to another.
#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    /// Where it leaves its source, just outside the card.
    pub start: (u16, u16),
    /// Where it reaches its target, just outside the card.
    pub end: (u16, u16),
    pub confidence: Confidence,
    pub in_cycle: bool,
    /// The head at the end, such as `>`.
    pub head: char,
}

/// How each relation between two cards that show is routed. Relations within one
/// column are left out, since they would run through the cards.
#[must_use]
pub fn routes(scene: &Scene, edges: &[Edge]) -> Vec<Route> {
    let mut found = Vec::new();
    for edge in edges {
        let (Some(from), Some(to)) = (scene.card(&edge.from), scene.card(&edge.to)) else {
            continue;
        };
        if from.column == to.column {
            continue;
        }
        let row_in = |card: &Card, members: &[String]| {
            members
                .iter()
                .find_map(|name| card.row_of(&card.id.child(name)))
                .unwrap_or(card.area.y + 1)
        };
        let from_y = row_in(from, &edge.from_members);
        let to_y = row_in(to, &edge.to_members);
        let rightwards = from.area.x < to.area.x;
        let (start, end) = if rightwards {
            (
                (from.area.x + from.area.width, from_y),
                (to.area.x.saturating_sub(1), to_y),
            )
        } else {
            (
                (from.area.x.saturating_sub(1), from_y),
                (to.area.x + to.area.width, to_y),
            )
        };
        let head = match edge.kind {
            cyclops_domain::relation::RelationKind::Inherits => {
                if rightwards {
                    '▷'
                } else {
                    '◁'
                }
            }
            cyclops_domain::relation::RelationKind::Implements => '○',
            cyclops_domain::relation::RelationKind::Composes => '◆',
            _ => {
                if rightwards {
                    '>'
                } else {
                    '<'
                }
            }
        };
        found.push(Route {
            start,
            end,
            confidence: edge.confidence,
            in_cycle: edge.in_cycle,
            head,
        });
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use cyclops_domain::symbol::SymbolKind;

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

    fn placed(id: &str, column: usize, row: usize) -> Placed {
        Placed {
            id: SymbolId::new(id),
            column,
            row,
        }
    }

    #[test]
    fn a_column_that_does_not_fit_folds_into_a_card() {
        let ids: Vec<String> = (1..=12).map(|index| format!("m{index:02}")).collect();
        let mut nodes: Vec<Node> = ids.iter().map(|id| node(id)).collect();
        nodes.push(node("hub"));
        let mut layout: Vec<Placed> = ids
            .iter()
            .enumerate()
            .map(|(row, id)| placed(id, 0, row))
            .collect();
        layout.push(placed("hub", 1, 0));
        let view = DiagramView {
            nodes,
            edges: Vec::new(),
        };
        let expanded = HashSet::new();
        let setting = Setting {
            level: Level::Module,
            focus: None,
            expanded: &expanded,
            promoted: None,
        };
        let small = lay_out(&view, &layout, &setting, Rect::new(0, 0, 80, 14));
        let more = small.more.first().expect("a more card");
        assert_eq!(
            small.cards.iter().filter(|card| card.column == 0).count(),
            2
        );
        assert_eq!(more.hidden.len(), 10);
        let large = lay_out(&view, &layout, &setting, Rect::new(0, 0, 200, 70));
        assert_eq!(large.more, Vec::new());
        assert_eq!(large.cards.len(), 13);
    }

    #[test]
    fn a_class_box_lists_fields_then_methods_worst_first() {
        let mut order = node("shop::orders::Order");
        order.kind = SymbolKind::Struct;
        let member = |name: &str, overlay| Member {
            name: name.to_owned(),
            visibility: Visibility::Public,
            overlay,
            alerted: false,
        };
        order.fields = vec![member("invoices", None)];
        order.methods = vec![member("add", Some(1.0)), member("total", Some(3.0))];
        let rows = members(&order, usize::MAX);
        let texts: Vec<&str> = rows
            .iter()
            .filter_map(|row| match row {
                BoxRow::Member { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(texts, vec!["+ invoices", "+ total()", "+ add()"]);
        assert_eq!(members(&order, 1).last(), Some(&BoxRow::More(1)));
    }
}

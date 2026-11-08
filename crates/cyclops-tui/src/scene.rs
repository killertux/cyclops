//! Laying out a diagram on a grid of cells: where each node and class box goes, which
//! nodes fold into a `+N more` card, and how each relation is routed. Both the keys and
//! the drawing use it, so what moves is what is shown.

use crate::app::Placed;
use cyclops_application::diagram::{DiagramView, Edge, Level, Member, Node};
use cyclops_domain::relation::Confidence;
use cyclops_domain::symbol::{SymbolId, Visibility};
use ratatui::layout::Rect;
use std::collections::{BTreeMap, HashSet};

/// The width of a node at the design size: a module's card, and a class box.
const CARD_WIDTH: u16 = 20;
const BOX_WIDTH: u16 = 24;
/// The columns left of the first node, and the fewest between two columns of nodes.
const CARD_MARGIN: u16 = 2;
const BOX_MARGIN: u16 = 1;
const LEAST_GAP: u16 = 2;
/// The rows a module's card takes with the rows below it: its border, its name, and the
/// overlay metric, then a gap of two.
const CARD_HEIGHT: u16 = 4;
const SLOT: u16 = 6;
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
    let boxes = setting.level == Level::Type;
    let (wanted, margin) = if boxes {
        (BOX_WIDTH, BOX_MARGIN)
    } else {
        (CARD_WIDTH, CARD_MARGIN)
    };
    let count = u16::try_from(columns.len()).unwrap_or(1).max(1);
    // The width the nodes leave goes between the columns.
    let spare = area.width.saturating_sub(wanted * count + 2 * margin);
    let gap = if count > 1 {
        (spare / (count - 1)).max(LEAST_GAP)
    } else {
        0
    };
    let width = wanted
        .min(area.width.saturating_sub(gap * (count - 1)) / count)
        .max(10);
    let used = width * count + gap * (count - 1);
    let left = area.x + area.width.saturating_sub(used) / 2;
    if !boxes {
        return cards(view, &columns, setting, area, (left, width, gap));
    }
    let bottom = area.y + area.height;
    let mut scene = Scene::default();
    for (index, (column, ids)) in columns.iter().enumerate() {
        let x = left + u16::try_from(index).unwrap_or(0) * (width + gap);
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
            let shown = if setting.expanded.contains(*id) {
                usize::MAX
            } else if setting.focus == Some(*id) {
                FOCUS_ROWS
            } else {
                NEIGHBOR_ROWS
            };
            let rows = members(node, shown);
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

/// Lays out cards of modules or packages: each column in slots of the same height,
/// centered against the fullest column, folding what does not fit into a `+N more` card.
fn cards(
    view: &DiagramView,
    columns: &BTreeMap<usize, Vec<&SymbolId>>,
    setting: &Setting<'_>,
    area: Rect,
    (left, width, gap): (u16, u16, u16),
) -> Scene {
    let known: HashSet<&SymbolId> = view.nodes.iter().map(|node| &node.id).collect();
    // The first row is for the columns' labels. Cards are two rows apart, or one if that
    // is all it takes for every card to fit.
    let slots_of =
        |slot: u16| usize::from((area.height.saturating_sub(1) + slot - CARD_HEIGHT) / slot).max(1);
    let longest = columns.values().map(Vec::len).max().unwrap_or(0);
    let slot = if longest > slots_of(SLOT) && longest <= slots_of(SLOT - 1) {
        SLOT - 1
    } else {
        SLOT
    };
    let slots = slots_of(slot);
    let shown = |ids: &Vec<&SymbolId>| ids.len().min(slots);
    let fullest = columns.values().map(shown).max().unwrap_or(1);
    let mut scene = Scene::default();
    for (index, (column, ids)) in columns.iter().enumerate() {
        let x = left + u16::try_from(index).unwrap_or(0) * (width + gap);
        let mut ids: Vec<&SymbolId> = ids
            .iter()
            .copied()
            .filter(|id| known.contains(id))
            .collect();
        if let Some(promoted) = setting.promoted
            && let Some(at) = ids.iter().position(|id| *id == promoted)
        {
            let chosen = ids.remove(at);
            ids.insert(0, chosen);
        }
        let folded = ids.len() > slots;
        let kept = if folded { slots - 1 } else { ids.len() };
        let offset = (fullest - shown(&ids)) / 2;
        let top = |at: usize| {
            area.y
                + 1
                + u16::try_from(at + offset)
                    .unwrap_or(u16::MAX)
                    .saturating_mul(slot)
        };
        for (at, id) in ids.iter().take(kept).enumerate() {
            scene.cards.push(Card {
                id: (*id).clone(),
                column: *column,
                area: Rect {
                    x,
                    y: top(at),
                    width,
                    height: CARD_HEIGHT,
                },
                rows: Vec::new(),
            });
        }
        if folded {
            scene.more.push(MoreCard {
                column: *column,
                area: Rect {
                    x,
                    y: top(kept),
                    width,
                    height: 3,
                },
                hidden: ids.iter().skip(kept).map(|id| (*id).clone()).collect(),
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
        // Fourteen rows hold the labels and two slots of six: one card, then the rest.
        let small = lay_out(&view, &layout, &setting, Rect::new(0, 0, 80, 14));
        let more = small.more.first().expect("a more card");
        assert_eq!(
            small.cards.iter().filter(|card| card.column == 0).count(),
            1
        );
        assert_eq!(more.hidden.len(), 11);
        let large = lay_out(&view, &layout, &setting, Rect::new(0, 0, 200, 70));
        assert_eq!(large.more, Vec::new());
        assert_eq!(large.cards.len(), 13);
    }

    #[test]
    fn columns_stand_where_the_design_has_them() {
        let ids = ["left", "focus", "right"];
        let view = DiagramView {
            nodes: ids.iter().map(|id| node(id)).collect(),
            edges: Vec::new(),
        };
        let layout: Vec<Placed> = ids
            .iter()
            .enumerate()
            .map(|(column, id)| placed(id, column, 0))
            .collect();
        let expanded = HashSet::new();
        let xs = |level: Level| {
            let setting = Setting {
                level,
                focus: None,
                expanded: &expanded,
                promoted: None,
            };
            // Beside an inspector of 36, the diagram has 82 columns inside its border.
            lay_out(&view, &layout, &setting, Rect::new(1, 5, 82, 24))
                .cards
                .iter()
                .map(|card| (card.area.x, card.area.width))
                .collect::<Vec<_>>()
        };
        assert_eq!(xs(Level::Module), vec![(3, 20), (32, 20), (61, 20)]);
        assert_eq!(xs(Level::Type), vec![(2, 24), (30, 24), (58, 24)]);
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

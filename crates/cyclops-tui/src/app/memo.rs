//! What the views work out from the analysis and the lens, kept from one frame to the
//! next. A large project takes long to rank, sort and lay out, while moving the cursor
//! changes none of it, so each value is kept with what it was worked out from, and
//! worked out again only once that changes.

use super::{Drawing, MapScene};
use cyclops_application::diagram::Level;
use cyclops_domain::filter::Filter;
use cyclops_domain::metric::{MetricId, Subject};
use cyclops_domain::relation::RelationKind;
use cyclops_domain::symbol::SymbolId;
use ratatui::layout::Rect;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

/// One value, and the key it was worked out for.
pub(super) struct Memo<K, V>(Cell<Option<(K, V)>>);

impl<K, V> Default for Memo<K, V> {
    fn default() -> Self {
        Self(Cell::new(None))
    }
}

impl<K: PartialEq, V: Clone> Memo<K, V> {
    /// The value for a key: the one kept, if it was worked out for the same key, or else
    /// a new one from `make`, which is kept in its place.
    pub(super) fn get(&self, key: K, make: impl FnOnce() -> V) -> V {
        let value = match self.0.take() {
            Some((known, value)) if known == key => value,
            _ => make(),
        };
        self.0.set(Some((key, value.clone())));
        value
    }
}

/// What the diagram is worked out from.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct DiagramKey {
    pub analysis: usize,
    pub level: Level,
    pub focus: Option<SymbolId>,
    pub depth: u32,
    pub relations: Vec<RelationKind>,
    pub overlay: Option<MetricId>,
    pub filter: Option<Filter>,
}

/// What Find's list is worked out from: the analysis, the metric it sorts by, whether
/// lowest first, and the filter.
pub(super) type FoundKey = (usize, MetricId, bool, Option<Filter>);

/// What the map is worked out from: the analysis, the focus, the metric, the filter and
/// the area it fills.
pub(super) type MapKey = (usize, Option<SymbolId>, MetricId, Option<Filter>, Rect);

/// What a filter keeps of the hierarchy, if there is a filter.
pub(super) type Kept = Option<Rc<HashSet<SymbolId>>>;

/// The rows of the Explore tree, each as the symbol's place in the model, its depth and
/// whether it is open.
pub(super) type TreeRows = Rc<[(usize, usize, Option<bool>)]>;

/// Everything the views keep.
#[derive(Default)]
pub(super) struct Memos {
    /// The symbols Find lists, as places in the model, for the analysis, the metric it
    /// sorts by, whether lowest first, and the filter.
    pub found: Memo<FoundKey, Rc<[usize]>>,
    /// The rows of the tree, for the analysis, the metric, the filter and what is open.
    pub tree: Memo<(usize, MetricId, Option<Filter>, HashSet<SymbolId>), TreeRows>,
    /// What a filter keeps of the hierarchy, for the analysis and the filter.
    pub kept: Memo<(usize, Option<Filter>), Kept>,
    /// The marks of the tree, for the analysis.
    pub tree_marks: Memo<usize, Rc<HashMap<SymbolId, String>>>,
    /// The marks of the diagram, for the analysis.
    pub diagram_marks: Memo<usize, Rc<HashMap<SymbolId, String>>>,
    /// The Overview's ranking, for the analysis, the metric and the filter.
    pub ranking: Memo<(usize, MetricId, Option<Filter>), Rc<[Subject]>>,
    pub drawing: Memo<DiagramKey, Option<Rc<Drawing>>>,
    /// How many nodes the diagram would have without the filter.
    pub unfiltered_nodes: Memo<DiagramKey, usize>,
    /// The map, for the analysis, the focus, the metric, the filter and its area.
    pub map: Memo<MapKey, Rc<MapScene>>,
}

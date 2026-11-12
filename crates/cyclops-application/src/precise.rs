//! Precise analysis: what the compilers' indexers say, laid over the relations the
//! syntax layer guessed. It only upgrades relations. A guess the index proves becomes
//! exact, a guess it disproves goes away, and a file that changed since it was indexed
//! keeps the syntax layer's guesses until the next precise analysis.

use crate::ports::{
    ContentHash, FactStore, IndexCommit, IndexError, Progress, SemanticIndex, SemanticProvider,
    Step, StoredIndex,
};
use cyclops_domain::language::LanguageId;
use cyclops_domain::path::RelativePath;
use cyclops_domain::problem::Problem;
use cyclops_domain::relation::{Confidence, Relation};
use cyclops_domain::settings::SemanticMode;
use cyclops_domain::symbol::{Symbol, SymbolId};
use std::collections::{BTreeSet, HashMap, HashSet};

/// Whether this analysis asks for the indexers, and which ones there are.
#[derive(Clone, Copy, Default)]
pub struct Precise<'a> {
    pub providers: &'a [Box<dyn SemanticProvider>],
    /// Asked for this time, such as with `cyclops analyze --precise`.
    pub asked: bool,
}

/// What an analysis knows about precise analysis.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PreciseStatus {
    /// The labels of the indexers this analysis ran.
    pub ran: Vec<String>,
    /// Whether every index still matches the code, or nothing if there is no index.
    pub current: Option<bool>,
    /// How many files of the indexed languages changed, appeared or went since.
    pub changed: usize,
    /// The commit the indexes were made at, if it is known.
    pub commit: Option<IndexCommit>,
}

/// A file as the analysis found it.
pub(crate) struct CurrentFile<'a> {
    pub path: &'a RelativePath,
    pub language: LanguageId,
    pub hash: ContentHash,
}

/// Where the symbols of each file are, to find the symbol around a line.
struct Places<'a> {
    /// The symbols of each file with each name, in order.
    by_name: HashMap<(&'a RelativePath, &'a str), Vec<&'a Symbol>>,
    /// For each file, the innermost symbol around each line, by line number.
    innermost: HashMap<&'a RelativePath, Vec<Option<&'a Symbol>>>,
}

/// The lines a symbol covers, and how many more after the first.
fn lines_of(symbol: &Symbol) -> Option<(u32, u32, u32)> {
    let span = symbol.location.as_ref()?.span;
    let size = span.end_line.checked_sub(span.start_line)?;
    Some((span.start_line, span.end_line, size))
}

impl<'a> Places<'a> {
    fn new(symbols: &'a [Symbol]) -> Self {
        let mut by_file: HashMap<&RelativePath, Vec<&Symbol>> = HashMap::new();
        let mut by_name: HashMap<(&RelativePath, &str), Vec<&Symbol>> = HashMap::new();
        for symbol in symbols {
            if let Some(location) = &symbol.location {
                by_file.entry(&location.file).or_default().push(symbol);
                by_name
                    .entry((&location.file, symbol.name.as_str()))
                    .or_default()
                    .push(symbol);
            }
        }
        let innermost = by_file
            .into_iter()
            .map(|(file, symbols)| (file, innermost_by_line(&symbols)))
            .collect();
        Self { by_name, innermost }
    }

    /// The innermost symbol whose lines hold `line`, and that has this name if one is
    /// given.
    fn around(&self, path: &RelativePath, line: u32, name: Option<&str>) -> Option<&'a Symbol> {
        let Some(name) = name else {
            let line = usize::try_from(line).ok()?;
            return self.innermost.get(path)?.get(line).copied().flatten();
        };
        self.by_name
            .get(&(path, name))?
            .iter()
            .filter_map(|symbol| {
                let (start, end, size) = lines_of(symbol)?;
                (start <= line && line <= end).then_some((size, *symbol))
            })
            .min_by(|(a, one), (b, other)| {
                a.cmp(b)
                    .then_with(|| other.id.as_str().len().cmp(&one.id.as_str().len()))
            })
            .map(|(_, symbol)| symbol)
    }
}

/// For each line of a file, the innermost of its symbols around it: the one that covers
/// the fewest lines, then the one with the longest id, then the first.
fn innermost_by_line<'a>(symbols: &[&'a Symbol]) -> Vec<Option<&'a Symbol>> {
    let mut spans: Vec<(usize, u32, u32, u32, &Symbol)> = symbols
        .iter()
        .enumerate()
        .filter_map(|(order, symbol)| {
            let (start, end, size) = lines_of(symbol)?;
            Some((order, start, end, size, *symbol))
        })
        .collect();
    // The one that should win is laid down last, over the others.
    spans.sort_by(|(order_a, _, _, size_a, a), (order_b, _, _, size_b, b)| {
        size_b
            .cmp(size_a)
            .then_with(|| a.id.as_str().len().cmp(&b.id.as_str().len()))
            .then_with(|| order_b.cmp(order_a))
    });
    let last = spans
        .iter()
        .map(|(_, _, end, _, _)| *end)
        .max()
        .unwrap_or(0);
    let mut lines = vec![None; usize::try_from(last).unwrap_or(0).saturating_add(1)];
    for (_, start, end, _, symbol) in spans {
        let from = usize::try_from(start).unwrap_or(usize::MAX);
        let to = usize::try_from(end).unwrap_or(0);
        for slot in lines.iter_mut().take(to.saturating_add(1)).skip(from) {
            *slot = Some(symbol);
        }
    }
    lines
}

/// Which symbol uses which, according to an index: the innermost symbol around each use,
/// and the symbol it uses. Names the project does not define, such as those of the
/// standard library, are left out.
#[must_use]
pub fn resolve(index: &SemanticIndex, symbols: &[Symbol]) -> Vec<(SymbolId, SymbolId)> {
    let places = Places::new(symbols);
    let mut defined: HashMap<&str, &SymbolId> = HashMap::new();
    for document in &index.documents {
        for definition in &document.definitions {
            if let Some(symbol) =
                places.around(&document.path, definition.line, Some(&definition.name))
            {
                defined.entry(definition.key.as_str()).or_insert(&symbol.id);
            }
        }
    }
    let mut uses = BTreeSet::new();
    for document in &index.documents {
        for reference in &document.references {
            let target = defined.get(reference.key.as_str()).or_else(|| {
                reference
                    .owner
                    .as_deref()
                    .and_then(|owner| defined.get(owner))
            });
            let (Some(target), Some(from)) =
                (target, places.around(&document.path, reference.line, None))
            else {
                continue;
            };
            uses.insert((from.id.clone(), (*target).clone()));
        }
    }
    uses.into_iter().collect()
}

/// How many files of an index's languages changed, appeared or went since it was made.
fn changed_since(index: &StoredIndex, files: &[CurrentFile<'_>]) -> usize {
    let recorded: HashMap<&RelativePath, &ContentHash> = index
        .files
        .iter()
        .map(|(path, hash)| (path, hash))
        .collect();
    let theirs: Vec<&CurrentFile<'_>> = files
        .iter()
        .filter(|file| index.languages.contains(&file.language))
        .collect();
    let differ = theirs
        .iter()
        .filter(|file| recorded.get(file.path) != Some(&&file.hash))
        .count();
    let gone = recorded
        .keys()
        .filter(|path| !theirs.iter().any(|file| file.path == **path))
        .count();
    differ + gone
}

/// Whether an index still matches the files of its languages.
fn is_current(index: &StoredIndex, files: &[CurrentFile<'_>]) -> bool {
    let recorded: HashMap<&RelativePath, &ContentHash> = index
        .files
        .iter()
        .map(|(path, hash)| (path, hash))
        .collect();
    let mut seen = 0;
    for file in files
        .iter()
        .filter(|file| index.languages.contains(&file.language))
    {
        if recorded.get(file.path) != Some(&&file.hash) {
            return false;
        }
        seen += 1;
    }
    seen == recorded.len()
}

/// The project's relations, with the guesses the indexes prove made exact and the ones
/// they disprove left out. Relations from files that changed since they were indexed
/// stay as the syntax layer found them.
#[must_use]
pub(crate) fn enrich(
    relations: Vec<Relation>,
    symbols: &[Symbol],
    indexes: &[StoredIndex],
    files: &[CurrentFile<'_>],
) -> Vec<Relation> {
    let hashes: HashMap<&RelativePath, &ContentHash> =
        files.iter().map(|file| (file.path, &file.hash)).collect();
    let mut current: HashSet<&RelativePath> = HashSet::new();
    for index in indexes {
        let recorded: HashMap<&RelativePath, &ContentHash> = index
            .files
            .iter()
            .map(|(path, hash)| (path, hash))
            .collect();
        for path in &index.covered {
            if let (Some(then), Some(now)) = (recorded.get(path), hashes.get(path))
                && then == now
            {
                current.insert(path);
            }
        }
    }
    if current.is_empty() {
        return relations;
    }
    let tree = Tree::new(symbols);
    // What each symbol uses, by places.
    let mut used: Vec<Vec<usize>> = vec![Vec::new(); symbols.len()];
    for (from, to) in indexes.iter().flat_map(|index| &index.uses) {
        if let (Some(from), Some(to)) = (tree.place(from), tree.place(to))
            && let Some(targets) = used.get_mut(from)
        {
            targets.push(to);
        }
    }
    // The guesses to check, by the symbol they start from, so that what a symbol uses is
    // worked out once for all of them.
    let mut checked: HashMap<usize, Vec<usize>> = HashMap::new();
    for (at, relation) in relations.iter().enumerate() {
        if relation.confidence == Confidence::Exact {
            continue;
        }
        let Some(from) = tree.place(&relation.from) else {
            continue;
        };
        if tree
            .file_of(from)
            .is_some_and(|file| current.contains(file))
        {
            checked.entry(from).or_default().push(at);
        }
    }
    let mut proven: Vec<Option<bool>> = vec![None; relations.len()];
    for (from, guesses) in checked {
        let reached = tree.reached(from, &used);
        for at in guesses {
            let target = relations
                .get(at)
                .and_then(|relation| tree.place(&relation.to));
            if let Some(verdict) = proven.get_mut(at) {
                *verdict = Some(target.is_some_and(|target| reached.contains(&target)));
            }
        }
    }
    relations
        .into_iter()
        .zip(proven)
        .filter_map(|(mut relation, verdict)| match verdict {
            None => Some(relation),
            Some(true) => {
                relation.confidence = Confidence::Exact;
                Some(relation)
            }
            Some(false) => None,
        })
        .collect()
}

/// How symbols nest, by their places in the list of symbols.
struct Tree<'a> {
    symbols: &'a [Symbol],
    places: HashMap<&'a SymbolId, usize>,
    parents: Vec<Option<usize>>,
    children: Vec<Vec<usize>>,
}

impl<'a> Tree<'a> {
    fn new(symbols: &'a [Symbol]) -> Self {
        let places: HashMap<&SymbolId, usize> = symbols
            .iter()
            .enumerate()
            .map(|(place, symbol)| (&symbol.id, place))
            .collect();
        let parents: Vec<Option<usize>> = symbols
            .iter()
            .map(|symbol| {
                symbol
                    .parent
                    .as_ref()
                    .and_then(|parent| places.get(parent))
                    .copied()
            })
            .collect();
        let mut children = vec![Vec::new(); symbols.len()];
        for (place, parent) in parents.iter().enumerate() {
            if let Some(held) = parent.and_then(|parent| children.get_mut(parent)) {
                held.push(place);
            }
        }
        Self {
            symbols,
            places,
            parents,
            children,
        }
    }

    fn place(&self, id: &SymbolId) -> Option<usize> {
        self.places.get(id).copied()
    }

    fn file_of(&self, place: usize) -> Option<&'a RelativePath> {
        self.symbols
            .get(place)?
            .location
            .as_ref()
            .map(|location| &location.file)
    }

    /// Everything that a symbol, or anything inside it, uses according to the indexes,
    /// with everything that holds what it uses. A guess from the symbol is proven when
    /// its target is among them.
    fn reached(&self, from: usize, used: &[Vec<usize>]) -> HashSet<usize> {
        let mut reached = HashSet::new();
        let mut pending = vec![from];
        while let Some(at) = pending.pop() {
            for target in used.get(at).into_iter().flatten() {
                let mut next = Some(*target);
                while let Some(holder) = next {
                    if !reached.insert(holder) {
                        break;
                    }
                    next = self.parents.get(holder).copied().flatten();
                }
            }
            pending.extend(self.children.get(at).into_iter().flatten());
        }
        reached
    }
}

/// Runs the indexers if the settings and the request say so, and returns every index
/// there is: the new ones, and the ones the cache kept for the other languages.
/// What an analysis found that precise analysis works from.
#[derive(Clone, Copy)]
pub(crate) struct Found<'a> {
    pub symbols: &'a [Symbol],
    /// The source files analyzed.
    pub files: &'a [CurrentFile<'a>],
    /// Every file of the project, for the indexers to find what says how to build it.
    pub project: &'a [RelativePath],
}

pub(crate) fn run(
    precise: Precise<'_>,
    mode: SemanticMode,
    found: &Found<'_>,
    cache: &mut Option<&mut dyn FactStore>,
    problems: &mut Vec<Problem>,
    progress: &dyn Progress,
) -> Option<(Vec<StoredIndex>, PreciseStatus)> {
    let mut indexes = match cache.as_deref_mut().map(<dyn FactStore>::load_indexes) {
        Some(Ok(indexes)) => indexes,
        Some(Err(error)) => {
            problems.push(Problem::CacheUnavailable(error.to_string()));
            Vec::new()
        }
        None => Vec::new(),
    };
    let wanted = match mode {
        SemanticMode::Off => {
            if precise.asked {
                problems.push(Problem::PreciseOff);
            }
            false
        }
        SemanticMode::Manual => precise.asked,
        SemanticMode::Always => true,
    };
    let Found {
        symbols,
        files,
        project,
    } = *found;
    let mut ran = Vec::new();
    if wanted {
        for provider in precise.providers {
            let languages = provider.languages();
            if !files.iter().any(|file| languages.contains(&file.language)) {
                continue;
            }
            progress.indexing(provider.indexer());
            if !progress.step(Step::Indexing) {
                return None;
            }
            if let Some(index) = index_with(provider.as_ref(), symbols, files, project, problems) {
                if let Some(store) = cache.as_deref_mut()
                    && let Err(error) = store.save_index(&index)
                {
                    problems.push(Problem::CacheUnavailable(error.to_string()));
                }
                ran.push(index.label.clone());
                indexes.retain(|kept| kept.label != index.label);
                indexes.push(index);
            }
        }
    }
    let current =
        (!indexes.is_empty()).then(|| indexes.iter().all(|index| is_current(index, files)));
    let changed = indexes
        .iter()
        .map(|index| changed_since(index, files))
        .sum();
    let commit = indexes.iter().find_map(|index| index.commit.clone());
    Some((
        indexes,
        PreciseStatus {
            ran,
            current,
            changed,
            commit,
        },
    ))
}

/// Runs one indexer, if the project has any of its languages. A project it could not
/// index is a problem, and the others still count, unless none could be indexed.
fn index_with(
    provider: &dyn SemanticProvider,
    symbols: &[Symbol],
    files: &[CurrentFile<'_>],
    project: &[RelativePath],
    problems: &mut Vec<Problem>,
) -> Option<StoredIndex> {
    let languages = provider.languages();
    let theirs: Vec<(RelativePath, ContentHash)> = files
        .iter()
        .filter(|file| languages.contains(&file.language))
        .map(|file| (file.path.clone(), file.hash))
        .collect();
    if theirs.is_empty() {
        return None;
    }
    let language = provider.label().to_owned();
    let indexed = match provider
        .availability()
        .and_then(|()| provider.index(project))
    {
        Ok(indexed) => indexed,
        Err(IndexError::Failed(reason)) => {
            problems.push(Problem::PreciseFailed {
                language,
                folder: None,
                reason,
            });
            return None;
        }
        Err(error) => {
            problems.push(Problem::PreciseSkipped {
                language,
                reason: error.to_string(),
            });
            return None;
        }
    };
    let failed = !indexed.failures.is_empty();
    for (folder, reason) in indexed.failures {
        problems.push(Problem::PreciseFailed {
            language: language.clone(),
            folder: (!folder.as_str().is_empty()).then_some(folder),
            reason,
        });
    }
    // An index of nothing, because every project failed, would hide the last good one.
    if failed && indexed.index.documents.is_empty() {
        return None;
    }
    let index = indexed.index;
    Some(StoredIndex {
        label: language,
        languages: languages.to_vec(),
        files: theirs,
        covered: index
            .documents
            .iter()
            .map(|document| document.path.clone())
            .collect(),
        uses: resolve(&index, symbols),
        commit: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{Indexed, IndexedDefinition, IndexedDocument, IndexedReference};
    use cyclops_domain::relation::RelationKind;
    use cyclops_domain::symbol::{CodeRole, Location, Span, SymbolKind};

    fn symbol(id: &str, kind: SymbolKind, file: &str, lines: (u32, u32)) -> Symbol {
        let (parent, name) = id.rsplit_once("::").unwrap();
        let mut symbol = Symbol::container(
            SymbolId::new(id),
            Some(SymbolId::new(parent)),
            name,
            kind,
            CodeRole::Production,
        );
        symbol.location = Some(Location {
            file: RelativePath::new(file),
            span: Span {
                start_line: lines.0,
                end_line: lines.1,
            },
        });
        symbol
    }

    /// Two files: `Order::total` calls `invoice.total()` on line 5, and both types
    /// have a `total`.
    fn shop() -> Vec<Symbol> {
        vec![
            symbol("shop::orders", SymbolKind::Module, "src/orders.rs", (1, 9)),
            symbol(
                "shop::orders::Order",
                SymbolKind::Struct,
                "src/orders.rs",
                (1, 2),
            ),
            symbol(
                "shop::orders::Order::total",
                SymbolKind::Method,
                "src/orders.rs",
                (4, 6),
            ),
            symbol(
                "shop::billing",
                SymbolKind::Module,
                "src/billing.rs",
                (1, 9),
            ),
            symbol(
                "shop::billing::Invoice",
                SymbolKind::Struct,
                "src/billing.rs",
                (1, 2),
            ),
            symbol(
                "shop::billing::Invoice::total",
                SymbolKind::Method,
                "src/billing.rs",
                (4, 6),
            ),
        ]
    }

    const INVOICE_TOTAL: &str = "billing/Invoice#total().";

    fn shop_index() -> SemanticIndex {
        SemanticIndex {
            documents: vec![
                IndexedDocument {
                    path: RelativePath::new("src/billing.rs"),
                    definitions: vec![IndexedDefinition {
                        line: 4,
                        name: "total".to_owned(),
                        key: INVOICE_TOTAL.to_owned(),
                    }],
                    references: Vec::new(),
                },
                IndexedDocument {
                    path: RelativePath::new("src/orders.rs"),
                    definitions: Vec::new(),
                    references: vec![
                        IndexedReference {
                            line: 5,
                            key: INVOICE_TOTAL.to_owned(),
                            owner: None,
                        },
                        IndexedReference {
                            line: 5,
                            key: "std/Vec#".to_owned(),
                            owner: None,
                        },
                    ],
                },
            ],
        }
    }

    fn id(text: &str) -> SymbolId {
        SymbolId::new(text)
    }

    fn calls(from: &str, to: &str, confidence: Confidence) -> Relation {
        Relation {
            from: id(from),
            to: id(to),
            kind: RelationKind::Calls,
            confidence,
        }
    }

    fn file(path: &RelativePath, hash: u8) -> CurrentFile<'_> {
        CurrentFile {
            path,
            language: LanguageId::Rust,
            hash: [hash; 32],
        }
    }

    fn stored(uses: Vec<(SymbolId, SymbolId)>) -> StoredIndex {
        StoredIndex {
            label: "Rust".to_owned(),
            languages: vec![LanguageId::Rust],
            files: vec![
                (RelativePath::new("src/orders.rs"), [1; 32]),
                (RelativePath::new("src/billing.rs"), [2; 32]),
            ],
            covered: vec![
                RelativePath::new("src/orders.rs"),
                RelativePath::new("src/billing.rs"),
            ],
            uses,
            commit: None,
        }
    }

    #[test]
    fn the_innermost_symbol_around_a_line_is_the_smallest_then_the_longest_id_then_the_first() {
        let symbols = vec![
            symbol("c::m", SymbolKind::Module, "src/m.rs", (1, 20)),
            symbol("c::m::A", SymbolKind::Struct, "src/m.rs", (2, 10)),
            symbol("c::m::A::f", SymbolKind::Method, "src/m.rs", (4, 6)),
            symbol("c::m::g", SymbolKind::Function, "src/m.rs", (12, 14)),
            symbol("c::m::A::g", SymbolKind::Method, "src/m.rs", (12, 14)),
            symbol("c::m::h", SymbolKind::Function, "src/m.rs", (16, 17)),
            symbol("c::m::i", SymbolKind::Function, "src/m.rs", (16, 17)),
        ];
        let places = Places::new(&symbols);
        let file = RelativePath::new("src/m.rs");
        let around = |line: u32| {
            places
                .around(&file, line, None)
                .map(|symbol| symbol.id.as_str().to_owned())
        };
        assert_eq!(around(1).as_deref(), Some("c::m"));
        assert_eq!(around(3).as_deref(), Some("c::m::A"));
        assert_eq!(around(5).as_deref(), Some("c::m::A::f"));
        assert_eq!(around(13).as_deref(), Some("c::m::A::g"));
        assert_eq!(around(16).as_deref(), Some("c::m::h"));
        assert_eq!(around(21), None);
        assert_eq!(
            places
                .around(&file, 13, Some("g"))
                .map(|symbol| symbol.id.as_str()),
            Some("c::m::A::g")
        );
        assert_eq!(places.around(&file, 3, Some("f")), None);
    }

    #[test]
    fn a_use_belongs_to_the_innermost_symbol_and_names_outside_the_project_are_left_out() {
        let uses = resolve(&shop_index(), &shop());
        assert_eq!(
            uses,
            vec![(
                id("shop::orders::Order::total"),
                id("shop::billing::Invoice::total")
            )]
        );
    }

    #[test]
    fn a_name_the_project_does_not_define_falls_back_to_its_owner() {
        let mut index = shop_index();
        let orders = index.documents.get_mut(1).unwrap();
        orders.references = vec![IndexedReference {
            line: 5,
            key: "billing/Invoice#<constructor>().".to_owned(),
            owner: Some("billing/Invoice#".to_owned()),
        }];
        let billing = index.documents.get_mut(0).unwrap();
        billing.definitions.push(IndexedDefinition {
            line: 1,
            name: "Invoice".to_owned(),
            key: "billing/Invoice#".to_owned(),
        });
        let uses = resolve(&index, &shop());
        assert_eq!(
            uses,
            vec![(
                id("shop::orders::Order::total"),
                id("shop::billing::Invoice")
            )]
        );
    }

    fn enriched(relations: Vec<Relation>, hashes: (u8, u8)) -> Vec<Relation> {
        let orders = RelativePath::new("src/orders.rs");
        let billing = RelativePath::new("src/billing.rs");
        let files = [file(&orders, hashes.0), file(&billing, hashes.1)];
        let index = stored(resolve(&shop_index(), &shop()));
        enrich(relations, &shop(), &[index], &files)
    }

    #[test]
    fn a_proven_guess_becomes_exact_and_a_disproven_one_goes_away() {
        let relations = vec![
            calls(
                "shop::orders::Order::total",
                "shop::billing::Invoice::total",
                Confidence::Ambiguous,
            ),
            calls(
                "shop::orders::Order::total",
                "shop::orders::Order::total",
                Confidence::Ambiguous,
            ),
        ];
        assert_eq!(
            enriched(relations, (1, 2)),
            vec![calls(
                "shop::orders::Order::total",
                "shop::billing::Invoice::total",
                Confidence::Exact,
            )]
        );
    }

    #[test]
    fn a_use_by_a_member_proves_a_use_by_what_holds_it() {
        let relations = vec![calls(
            "shop::orders",
            "shop::billing::Invoice",
            Confidence::Inferred,
        )];
        let found = enriched(relations, (1, 2));
        assert_eq!(
            found.first().map(|one| one.confidence),
            Some(Confidence::Exact)
        );
    }

    #[test]
    fn exact_relations_stay_whatever_the_index_says() {
        let relation = calls(
            "shop::orders::Order::total",
            "shop::orders::Order",
            Confidence::Exact,
        );
        assert_eq!(enriched(vec![relation.clone()], (1, 2)), vec![relation]);
    }

    #[test]
    fn a_file_that_changed_keeps_its_guesses() {
        let guess = calls(
            "shop::orders::Order::total",
            "shop::orders::Order::total",
            Confidence::Ambiguous,
        );
        assert_eq!(enriched(vec![guess.clone()], (9, 2)), vec![guess]);
        let proven = enriched(
            vec![calls(
                "shop::orders::Order::total",
                "shop::billing::Invoice::total",
                Confidence::Ambiguous,
            )],
            (1, 9),
        );
        assert_eq!(
            proven.first().map(|one| one.confidence),
            Some(Confidence::Exact)
        );
    }

    #[test]
    fn files_changed_since_an_index_are_counted() {
        let index = stored(Vec::new());
        let orders = RelativePath::new("src/orders.rs");
        let billing = RelativePath::new("src/billing.rs");
        let added = RelativePath::new("src/added.rs");
        assert_eq!(
            changed_since(&index, &[file(&orders, 1), file(&billing, 2)]),
            0
        );
        assert_eq!(
            changed_since(&index, &[file(&orders, 7), file(&billing, 2)]),
            1
        );
        assert_eq!(changed_since(&index, &[file(&orders, 1)]), 1);
        assert_eq!(
            changed_since(
                &index,
                &[file(&orders, 1), file(&billing, 2), file(&added, 3)]
            ),
            1
        );
    }

    #[test]
    fn an_index_is_current_until_a_file_of_its_languages_changes_or_appears() {
        let index = stored(Vec::new());
        let orders = RelativePath::new("src/orders.rs");
        let billing = RelativePath::new("src/billing.rs");
        let added = RelativePath::new("src/added.rs");
        let script = RelativePath::new("web/app.ts");
        let other = CurrentFile {
            path: &script,
            language: LanguageId::TypeScript,
            hash: [5; 32],
        };
        assert!(is_current(
            &index,
            &[file(&orders, 1), file(&billing, 2), other]
        ));
        assert!(!is_current(&index, &[file(&orders, 1), file(&billing, 3)]));
        assert!(!is_current(&index, &[file(&orders, 1)]));
        assert!(!is_current(
            &index,
            &[file(&orders, 1), file(&billing, 2), file(&added, 4)]
        ));
    }

    /// An indexer that answers from a script.
    struct Fake {
        missing: Option<&'static str>,
        fails: bool,
        /// Projects in folders that fail while the rest index.
        broken: &'static [(&'static str, &'static str)],
        /// Whether the projects that worked found anything.
        found: bool,
    }

    impl SemanticProvider for Fake {
        fn label(&self) -> &'static str {
            "Rust"
        }

        fn indexer(&self) -> &'static str {
            "rust-analyzer"
        }

        fn languages(&self) -> &[LanguageId] {
            &[LanguageId::Rust]
        }

        fn availability(&self) -> Result<(), IndexError> {
            self.missing
                .map_or(Ok(()), |tool| Err(IndexError::Missing(tool.to_owned())))
        }

        fn index(&self, _: &[RelativePath]) -> Result<Indexed, IndexError> {
            if self.fails {
                return Err(IndexError::Failed("it broke".to_owned()));
            }
            Ok(Indexed {
                index: if self.found {
                    shop_index()
                } else {
                    SemanticIndex::default()
                },
                failures: self
                    .broken
                    .iter()
                    .map(|(folder, reason)| (RelativePath::new(folder), (*reason).to_owned()))
                    .collect(),
            })
        }
    }

    fn run_with(
        fake: Fake,
        mode: SemanticMode,
        asked: bool,
    ) -> (Vec<StoredIndex>, PreciseStatus, Vec<Problem>) {
        let providers: Vec<Box<dyn SemanticProvider>> = vec![Box::new(fake)];
        let orders = RelativePath::new("src/orders.rs");
        let files = [file(&orders, 1)];
        let symbols = shop();
        let mut problems = Vec::new();
        let (indexes, status) = run(
            Precise {
                providers: &providers,
                asked,
            },
            mode,
            &Found {
                symbols: &symbols,
                files: &files,
                project: &[],
            },
            &mut None,
            &mut problems,
            &crate::ports::Silent,
        )
        .unwrap();
        (indexes, status, problems)
    }

    const WORKS: Fake = Fake {
        missing: None,
        fails: false,
        broken: &[],
        found: true,
    };

    #[test]
    fn indexers_run_when_asked_or_always_and_never_when_off() {
        let (indexes, status, problems) = run_with(WORKS, SemanticMode::Manual, true);
        assert_eq!(status.ran, vec!["Rust".to_owned()]);
        assert_eq!(status.current, Some(true));
        assert_eq!(indexes.first().map(|index| index.uses.len()), Some(1));
        assert_eq!(problems, Vec::<Problem>::new());

        let (_, status, problems) = run_with(WORKS, SemanticMode::Manual, false);
        assert_eq!(status.ran, Vec::<String>::new());
        assert_eq!(status.current, None);
        assert_eq!(problems, Vec::<Problem>::new());

        let (_, status, _) = run_with(WORKS, SemanticMode::Always, false);
        assert_eq!(status.ran.len(), 1);

        let (_, status, problems) = run_with(WORKS, SemanticMode::Off, true);
        assert_eq!(status.ran, Vec::<String>::new());
        assert_eq!(problems, vec![Problem::PreciseOff]);
    }

    #[test]
    fn a_missing_tool_or_a_failure_is_a_problem_and_never_fatal() {
        let missing = Fake {
            missing: Some("rust-analyzer"),
            ..WORKS
        };
        let (indexes, status, problems) = run_with(missing, SemanticMode::Manual, true);
        assert_eq!(indexes, Vec::<StoredIndex>::new());
        assert_eq!(status.ran, Vec::<String>::new());
        assert_eq!(
            problems.iter().map(ToString::to_string).collect::<Vec<_>>(),
            vec!["Precise analysis skipped for Rust: rust-analyzer not found.".to_owned()]
        );

        let broken = Fake {
            fails: true,
            ..WORKS
        };
        let (_, _, problems) = run_with(broken, SemanticMode::Manual, true);
        assert_eq!(
            problems.first().map(ToString::to_string).as_deref(),
            Some("Precise analysis failed for Rust: it broke")
        );
    }

    #[test]
    fn a_project_that_fails_leaves_the_others_and_names_its_folder() {
        let one_broken = Fake {
            broken: &[("services/api", "Error: no projects")],
            ..WORKS
        };
        let (indexes, status, problems) = run_with(one_broken, SemanticMode::Manual, true);
        assert_eq!(indexes.first().map(|index| index.uses.len()), Some(1));
        assert_eq!(status.ran, vec!["Rust".to_owned()]);
        assert_eq!(
            problems.iter().map(ToString::to_string).collect::<Vec<_>>(),
            vec!["Precise analysis failed for Rust in services/api: Error: no projects".to_owned()]
        );
    }

    #[test]
    fn when_every_project_fails_nothing_is_kept_and_the_root_has_no_folder() {
        let all_broken = Fake {
            broken: &[("", "Error: no projects")],
            found: false,
            ..WORKS
        };
        let (indexes, status, problems) = run_with(all_broken, SemanticMode::Manual, true);
        assert_eq!(indexes, Vec::<StoredIndex>::new());
        assert_eq!(status.ran, Vec::<String>::new());
        assert_eq!(
            problems.iter().map(ToString::to_string).collect::<Vec<_>>(),
            vec!["Precise analysis failed for Rust: Error: no projects".to_owned()]
        );
    }
}

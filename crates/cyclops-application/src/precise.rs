//! Precise analysis: what the compilers' indexers say, laid over the relations the
//! syntax layer guessed. It only upgrades relations. A guess the index proves becomes
//! exact, a guess it disproves goes away, and a file that changed since it was indexed
//! keeps the syntax layer's guesses until the next precise analysis.

use crate::ports::{ContentHash, FactStore, SemanticIndex, SemanticProvider, StoredIndex};
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
}

/// A file as the analysis found it.
pub(crate) struct CurrentFile<'a> {
    pub path: &'a RelativePath,
    pub language: LanguageId,
    pub hash: ContentHash,
}

/// Where the symbols of each file are, to find the symbol around a line.
struct Places<'a> {
    by_file: HashMap<&'a RelativePath, Vec<&'a Symbol>>,
}

impl<'a> Places<'a> {
    fn new(symbols: &'a [Symbol]) -> Self {
        let mut by_file: HashMap<&RelativePath, Vec<&Symbol>> = HashMap::new();
        for symbol in symbols {
            if let Some(location) = &symbol.location {
                by_file.entry(&location.file).or_default().push(symbol);
            }
        }
        Self { by_file }
    }

    /// The innermost symbol whose lines hold `line`, and that has this name if one is
    /// given.
    fn around(&self, path: &RelativePath, line: u32, name: Option<&str>) -> Option<&'a Symbol> {
        self.by_file
            .get(path)?
            .iter()
            .filter(|symbol| name.is_none_or(|name| symbol.name == name))
            .filter_map(|symbol| {
                let span = symbol.location.as_ref()?.span;
                (span.start_line <= line && line <= span.end_line)
                    .then_some((span.end_line - span.start_line, *symbol))
            })
            .min_by(|(a, one), (b, other)| {
                a.cmp(b)
                    .then_with(|| other.id.as_str().len().cmp(&one.id.as_str().len()))
            })
            .map(|(_, symbol)| symbol)
    }
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
    let mut uses: HashMap<&SymbolId, HashSet<&SymbolId>> = HashMap::new();
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
        for (from, to) in &index.uses {
            uses.entry(from).or_default().insert(to);
        }
    }
    if current.is_empty() {
        return relations;
    }
    let tree = Tree::new(symbols);
    relations
        .into_iter()
        .filter_map(|mut relation| {
            if relation.confidence == Confidence::Exact {
                return Some(relation);
            }
            let up_to_date = tree
                .file_of(&relation.from)
                .is_some_and(|file| current.contains(file));
            if !up_to_date {
                return Some(relation);
            }
            if tree.proves(&relation, &uses) {
                relation.confidence = Confidence::Exact;
                Some(relation)
            } else {
                None
            }
        })
        .collect()
}

/// How symbols nest.
struct Tree<'a> {
    symbols: HashMap<&'a SymbolId, &'a Symbol>,
    children: HashMap<&'a SymbolId, Vec<&'a SymbolId>>,
}

impl<'a> Tree<'a> {
    fn new(symbols: &'a [Symbol]) -> Self {
        let mut children: HashMap<&SymbolId, Vec<&SymbolId>> = HashMap::new();
        for symbol in symbols {
            if let Some(parent) = &symbol.parent {
                children.entry(parent).or_default().push(&symbol.id);
            }
        }
        Self {
            symbols: symbols.iter().map(|symbol| (&symbol.id, symbol)).collect(),
            children,
        }
    }

    fn file_of(&self, id: &SymbolId) -> Option<&'a RelativePath> {
        self.symbols
            .get(id)?
            .location
            .as_ref()
            .map(|location| &location.file)
    }

    /// Whether `ancestor` is `id` or holds it.
    fn holds(&self, ancestor: &SymbolId, id: &SymbolId) -> bool {
        let mut next = Some(id);
        while let Some(current) = next {
            if current == ancestor {
                return true;
            }
            next = self
                .symbols
                .get(current)
                .and_then(|symbol| symbol.parent.as_ref());
        }
        false
    }

    /// Whether something in the relation's source uses its target, or a part of it,
    /// according to the indexes.
    fn proves(&self, relation: &Relation, uses: &HashMap<&SymbolId, HashSet<&SymbolId>>) -> bool {
        let mut pending = vec![&relation.from];
        while let Some(id) = pending.pop() {
            if uses
                .get(id)
                .is_some_and(|targets| targets.iter().any(|to| self.holds(&relation.to, to)))
            {
                return true;
            }
            if let Some(children) = self.children.get(id) {
                pending.extend(children.iter().copied());
            }
        }
        false
    }
}

/// Runs the indexers if the settings and the request say so, and returns every index
/// there is: the new ones, and the ones the cache kept for the other languages.
pub(crate) fn run(
    precise: Precise<'_>,
    mode: SemanticMode,
    symbols: &[Symbol],
    files: &[CurrentFile<'_>],
    cache: &mut Option<&mut dyn FactStore>,
    problems: &mut Vec<Problem>,
) -> (Vec<StoredIndex>, PreciseStatus) {
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
    let mut ran = Vec::new();
    if wanted {
        for provider in precise.providers {
            if let Some(index) = index_with(provider.as_ref(), symbols, files, problems) {
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
    (indexes, PreciseStatus { ran, current })
}

/// Runs one indexer, if the project has any of its languages.
fn index_with(
    provider: &dyn SemanticProvider,
    symbols: &[Symbol],
    files: &[CurrentFile<'_>],
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
    let found = provider
        .availability()
        .map_err(|error| Problem::PreciseSkipped {
            language: language.clone(),
            reason: error.to_string(),
        });
    let found = found.and_then(|()| {
        provider.index().map_err(|error| Problem::PreciseFailed {
            language: language.clone(),
            reason: error.to_string(),
        })
    });
    match found {
        Ok(index) => Some(StoredIndex {
            label: language,
            languages: languages.to_vec(),
            files: theirs,
            covered: index
                .documents
                .iter()
                .map(|document| document.path.clone())
                .collect(),
            uses: resolve(&index, symbols),
        }),
        Err(problem) => {
            problems.push(problem);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{IndexError, IndexedDefinition, IndexedDocument, IndexedReference};
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
        }
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
    }

    impl SemanticProvider for Fake {
        fn label(&self) -> &'static str {
            "Rust"
        }

        fn languages(&self) -> &[LanguageId] {
            &[LanguageId::Rust]
        }

        fn availability(&self) -> Result<(), IndexError> {
            self.missing
                .map_or(Ok(()), |tool| Err(IndexError::Missing(tool.to_owned())))
        }

        fn index(&self) -> Result<SemanticIndex, IndexError> {
            if self.fails {
                Err(IndexError::Failed("it broke".to_owned()))
            } else {
                Ok(shop_index())
            }
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
        let mut problems = Vec::new();
        let (indexes, status) = run(
            Precise {
                providers: &providers,
                asked,
            },
            mode,
            &shop(),
            &files,
            &mut None,
            &mut problems,
        );
        (indexes, status, problems)
    }

    const WORKS: Fake = Fake {
        missing: None,
        fails: false,
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
            fails: false,
        };
        let (indexes, status, problems) = run_with(missing, SemanticMode::Manual, true);
        assert_eq!(indexes, Vec::<StoredIndex>::new());
        assert_eq!(status.ran, Vec::<String>::new());
        assert_eq!(
            problems.iter().map(ToString::to_string).collect::<Vec<_>>(),
            vec!["Precise analysis skipped for Rust: rust-analyzer not found.".to_owned()]
        );

        let broken = Fake {
            missing: None,
            fails: true,
        };
        let (_, _, problems) = run_with(broken, SemanticMode::Manual, true);
        assert_eq!(
            problems.first().map(ToString::to_string).as_deref(),
            Some("Precise analysis failed for Rust: it broke")
        );
    }
}

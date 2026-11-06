//! Extracts the facts of one Rust file.

use crate::body::{self, Function};
use crate::lines::LineMap;
use cyclops_domain::facts::{BodyFacts, FileFacts, Import, LineCounts, SymbolFact};
use cyclops_domain::language::LanguageId;
use cyclops_domain::path::RelativePath;
use cyclops_domain::symbol::{CodeRole, Span, SymbolKind, Visibility};
use ra_ap_syntax::ast::{self, AstNode, HasAttrs, HasModuleItem, HasName, HasVisibility};
use ra_ap_syntax::{Edition, NodeOrToken, SourceFile, SyntaxKind, SyntaxNode, TextSize};
use std::collections::VecDeque;

/// The parser recurses, so code nested deeply enough would overflow the stack and take
/// the whole analysis down. No person writes code near this deep, and the parser
/// handles about twice as much on the stack of a worker thread.
const MAX_BRACKET_DEPTH: usize = 256;

fn is_too_deep(text: &str) -> bool {
    let mut depth = 0_usize;
    for byte in text.bytes() {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
        if depth > MAX_BRACKET_DEPTH {
            return true;
        }
    }
    false
}

/// All that can be said about a file that was not parsed.
fn unparsed(path: &RelativePath, text: &str) -> FileFacts {
    let role = file_role(path);
    let lines = LineMap::unparsed(text).counts();
    FileFacts {
        path: path.clone(),
        language: LanguageId::Rust,
        role,
        lines,
        test_lines: match role {
            CodeRole::Test => lines,
            CodeRole::Production => LineCounts::default(),
        },
        symbols: Vec::new(),
        imports: Vec::new(),
        syntax_error: true,
    }
}

pub(crate) fn extract(path: &RelativePath, text: &str) -> FileFacts {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    if is_too_deep(text) {
        return unparsed(path, text);
    }
    let parse = SourceFile::parse(text, Edition::Edition2024);
    let tree = parse.tree();
    let lines = LineMap::new(text, tree.syntax());
    let role = file_role(path);

    let mut extractor = Extractor {
        lines: &lines,
        pending: VecDeque::new(),
        symbols: Vec::new(),
        imports: Vec::new(),
        test_lines: LineCounts::default(),
    };
    for item in tree.items() {
        extractor.pending.push_back(Pending::new(item, None, role));
    }
    while let Some(pending) = extractor.pending.pop_front() {
        extractor.item(pending);
    }

    let all_lines = lines.counts();
    FileFacts {
        path: path.clone(),
        language: LanguageId::Rust,
        role,
        lines: all_lines,
        test_lines: match role {
            CodeRole::Test => all_lines,
            CodeRole::Production => extractor.test_lines,
        },
        symbols: extractor.symbols,
        imports: extractor.imports,
        syntax_error: !parse.errors().is_empty(),
    }
}

/// `tests/` and `benches/` folders hold test code, unless they are inside `src/`.
fn file_role(path: &RelativePath) -> CodeRole {
    let mut folders = path.as_str().split('/').rev().skip(1).collect::<Vec<_>>();
    folders.reverse();
    let test_folder = folders
        .into_iter()
        .take_while(|folder| *folder != "src")
        .any(|folder| folder == "tests" || folder == "benches");
    if test_folder {
        CodeRole::Test
    } else {
        CodeRole::Production
    }
}

fn is_test_attribute(attribute: &ast::Attr) -> bool {
    let Some(meta) = attribute.meta() else {
        return false;
    };
    let text: String = meta
        .syntax()
        .text()
        .to_string()
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    if let Some(condition) = text.strip_prefix("cfg(") {
        let names_test = condition
            .split(|character: char| !character.is_alphanumeric() && character != '_')
            .any(|word| word == "test");
        return names_test && !condition.contains("not(");
    }
    let path = text.split('(').next().unwrap_or(&text);
    path == "test" || path.ends_with("::test")
}

fn has_test_attribute(item: &impl HasAttrs) -> bool {
    item.attrs().any(|attribute| is_test_attribute(&attribute))
}

fn visibility_of(item: &impl HasVisibility) -> Visibility {
    match item.visibility() {
        None => Visibility::Private,
        Some(visibility) if visibility.syntax().text() == "pub" => Visibility::Public,
        Some(_) => Visibility::Restricted,
    }
}

/// Where a node really starts, after its doc comments and attributes.
fn signature_start(node: &SyntaxNode) -> TextSize {
    node.children_with_tokens()
        .find(|element| {
            !matches!(
                element.kind(),
                SyntaxKind::ATTR
                    | SyntaxKind::DOC_COMMENT
                    | SyntaxKind::COMMENT
                    | SyntaxKind::WHITESPACE
            )
        })
        .map_or_else(
            || node.text_range().start(),
            |element| match element {
                NodeOrToken::Node(child) => child.text_range().start(),
                NodeOrToken::Token(token) => token.text_range().start(),
            },
        )
}

/// The names in the path of a type, leaving out references and generic arguments.
fn type_path(ty: &ast::Type) -> Vec<String> {
    let mut current = ty.clone();
    while let ast::Type::RefType(reference) = &current {
        match reference.ty() {
            Some(inner) => current = inner,
            None => break,
        }
    }
    if let ast::Type::PathType(path_type) = &current
        && let Some(path) = path_type.path()
    {
        let names: Vec<String> = path
            .segments()
            .filter_map(|segment| segment.name_ref())
            .map(|name| name.text().to_string())
            .collect();
        if !names.is_empty() {
            return names;
        }
    }
    vec![ty.syntax().text().to_string()]
}

/// An item still to extract, with what it inherits from around it.
struct Pending {
    item: ast::Item,
    parent: Option<usize>,
    role: CodeRole,
    /// The type it belongs to, if it comes from an `impl` block.
    owner: Option<Vec<String>>,
    /// What it inherits, when it has no visibility of its own.
    visibility: Option<Visibility>,
}

impl Pending {
    const fn new(item: ast::Item, parent: Option<usize>, role: CodeRole) -> Self {
        Self {
            item,
            parent,
            role,
            owner: None,
            visibility: None,
        }
    }
}

/// What every kind of symbol needs.
struct Described<'a> {
    name: String,
    kind: SymbolKind,
    node: &'a SyntaxNode,
    visibility: Visibility,
    role: CodeRole,
    parent: Option<usize>,
    owner: Option<Vec<String>>,
    body: Option<BodyFacts>,
}

struct Extractor<'a> {
    lines: &'a LineMap,
    pending: VecDeque<Pending>,
    symbols: Vec<SymbolFact>,
    imports: Vec<Import>,
    test_lines: LineCounts,
}

impl Extractor<'_> {
    fn item(&mut self, pending: Pending) {
        let inherited = pending.role;
        let test = match &pending.item {
            ast::Item::Module(item) => has_test_attribute(item),
            ast::Item::Fn(item) => has_test_attribute(item),
            ast::Item::Impl(item) => has_test_attribute(item),
            ast::Item::Struct(item) => has_test_attribute(item),
            ast::Item::Enum(item) => has_test_attribute(item),
            ast::Item::Trait(item) => has_test_attribute(item),
            ast::Item::Const(item) => has_test_attribute(item),
            ast::Item::Static(item) => has_test_attribute(item),
            ast::Item::TypeAlias(item) => has_test_attribute(item),
            ast::Item::Use(item) => has_test_attribute(item),
            _ => false,
        };
        let role = if test { CodeRole::Test } else { inherited };
        if role == CodeRole::Test && inherited == CodeRole::Production {
            self.count_test_lines(pending.item.syntax());
        }
        let pending = Pending { role, ..pending };
        match pending.item.clone() {
            ast::Item::Module(item) => self.module(&item, &pending),
            ast::Item::Fn(item) => self.function(&item, &pending),
            ast::Item::Struct(item) => self.structure(&item, &pending),
            ast::Item::Union(item) => self.union(&item, &pending),
            ast::Item::Enum(item) => {
                self.named(&item, SymbolKind::Enum, &pending);
            }
            ast::Item::Trait(item) => self.interface(&item, &pending),
            ast::Item::TypeAlias(item) if pending.owner.is_none() => {
                self.named(&item, SymbolKind::Alias, &pending);
            }
            ast::Item::Const(item) => {
                self.named(&item, SymbolKind::Constant, &pending);
            }
            ast::Item::Static(item) => {
                self.named(&item, SymbolKind::Constant, &pending);
            }
            ast::Item::Impl(item) => self.implementation(&item, &pending),
            ast::Item::Use(item) => self.import(&item),
            _ => {}
        }
    }

    fn count_test_lines(&mut self, node: &SyntaxNode) {
        let range = node.text_range();
        let start = self.lines.line_of(range.start());
        let end = self.lines.line_of(range.end() - TextSize::new(1));
        self.test_lines = self.test_lines.plus(self.lines.counts_between(start, end));
    }

    fn add(&mut self, symbol: Described<'_>) -> usize {
        let range = symbol.node.text_range();
        let start_line = self.lines.line_of(signature_start(symbol.node));
        let end_line = self.lines.line_of(
            range
                .end()
                .checked_sub(TextSize::new(1))
                .unwrap_or(range.end()),
        );
        self.symbols.push(SymbolFact {
            name: symbol.name,
            parent: symbol.parent,
            owner: symbol.owner,
            kind: symbol.kind,
            role: symbol.role,
            visibility: symbol.visibility,
            span: Span {
                start_line,
                end_line,
            },
            code_lines: self.lines.counts_between(start_line, end_line).code,
            body: symbol.body,
        });
        self.symbols.len() - 1
    }

    /// Adds a symbol that only has a name, and says where it went.
    fn named<T>(&mut self, item: &T, kind: SymbolKind, pending: &Pending) -> Option<usize>
    where
        T: HasName + HasVisibility,
    {
        let name = item.name()?.text().to_string();
        Some(
            self.add(Described {
                name,
                kind,
                node: item.syntax(),
                visibility: pending
                    .visibility
                    .filter(|_| item.visibility().is_none())
                    .unwrap_or_else(|| visibility_of(item)),
                role: pending.role,
                parent: pending.parent,
                owner: pending.owner.clone(),
                body: None,
            }),
        )
    }

    /// A `mod name;` is only a declaration, and its file speaks for itself.
    fn module(&mut self, item: &ast::Module, pending: &Pending) {
        let Some(list) = item.item_list() else {
            return;
        };
        let Some(index) = self.named(item, SymbolKind::Module, pending) else {
            return;
        };
        for inner in list.items() {
            self.pending
                .push_back(Pending::new(inner, Some(index), pending.role));
        }
    }

    fn belongs_to_a_type(&self, pending: &Pending) -> bool {
        pending.owner.is_some()
            || pending
                .parent
                .and_then(|parent| self.symbols.get(parent))
                .is_some_and(|parent| parent.kind.is_type())
    }

    fn function(&mut self, item: &ast::Fn, pending: &Pending) {
        let Some(name) = body::function_name(item) else {
            return;
        };
        let is_method = self.belongs_to_a_type(pending);
        let kind = if is_method {
            SymbolKind::Method
        } else {
            SymbolKind::Function
        };
        let walked = item.body().map(|block| {
            let function = Function {
                name: &name,
                is_method,
            };
            body::walk(self.lines, function, &block)
        });
        let (facts, nested) = walked.map_or((None, Vec::new()), |walked| {
            let parameters = item
                .param_list()
                .map_or(0, |list| list.params().fold(0, |count, _| count + 1));
            let body = BodyFacts {
                parameters,
                ..walked.body
            };
            (Some(body), walked.items)
        });
        let index = self.add(Described {
            name,
            kind,
            node: item.syntax(),
            visibility: pending
                .visibility
                .filter(|_| item.visibility().is_none())
                .unwrap_or_else(|| visibility_of(item)),
            role: pending.role,
            parent: pending.parent,
            owner: pending.owner.clone(),
            body: facts,
        });
        for inner in nested {
            self.pending
                .push_back(Pending::new(inner, Some(index), pending.role));
        }
    }

    fn structure(&mut self, item: &ast::Struct, pending: &Pending) {
        let Some(index) = self.named(item, SymbolKind::Struct, pending) else {
            return;
        };
        match item.field_list() {
            Some(ast::FieldList::RecordFieldList(fields)) => {
                self.record_fields(&fields, index, pending.role);
            }
            Some(ast::FieldList::TupleFieldList(fields)) => {
                for (position, field) in fields.fields().enumerate() {
                    self.add(Described {
                        name: position.to_string(),
                        kind: SymbolKind::Field,
                        node: field.syntax(),
                        visibility: visibility_of(&field),
                        role: pending.role,
                        parent: Some(index),
                        owner: None,
                        body: None,
                    });
                }
            }
            None => {}
        }
    }

    fn union(&mut self, item: &ast::Union, pending: &Pending) {
        if let Some(index) = self.named(item, SymbolKind::Struct, pending)
            && let Some(fields) = item.record_field_list()
        {
            self.record_fields(&fields, index, pending.role);
        }
    }

    fn record_fields(&mut self, fields: &ast::RecordFieldList, parent: usize, role: CodeRole) {
        for field in fields.fields() {
            let Some(name) = field.name() else {
                continue;
            };
            self.add(Described {
                name: name.text().to_string(),
                kind: SymbolKind::Field,
                node: field.syntax(),
                visibility: visibility_of(&field),
                role,
                parent: Some(parent),
                owner: None,
                body: None,
            });
        }
    }

    /// The items of a trait are as visible as the trait.
    fn interface(&mut self, item: &ast::Trait, pending: &Pending) {
        let Some(index) = self.named(item, SymbolKind::Interface, pending) else {
            return;
        };
        let visibility = visibility_of(item);
        for inner in Self::associated_items(item.assoc_item_list()) {
            self.pending.push_back(Pending {
                visibility: Some(visibility),
                ..Pending::new(inner, Some(index), pending.role)
            });
        }
    }

    /// The items of an `impl` belong to its type, which may be in another file. Those of
    /// a trait implementation are as visible as that type.
    fn implementation(&mut self, item: &ast::Impl, pending: &Pending) {
        let Some(owner) = item.self_ty().map(|ty| type_path(&ty)) else {
            return;
        };
        let visibility = item.trait_().map(|_| Visibility::Public);
        for inner in Self::associated_items(item.assoc_item_list()) {
            self.pending.push_back(Pending {
                owner: Some(owner.clone()),
                visibility,
                ..Pending::new(inner, pending.parent, pending.role)
            });
        }
    }

    fn associated_items(list: Option<ast::AssocItemList>) -> impl Iterator<Item = ast::Item> {
        list.into_iter()
            .flat_map(|list| list.assoc_items())
            .filter_map(|item| match item {
                ast::AssocItem::Fn(item) => Some(ast::Item::Fn(item)),
                ast::AssocItem::Const(item) => Some(ast::Item::Const(item)),
                ast::AssocItem::TypeAlias(item) => Some(ast::Item::TypeAlias(item)),
                ast::AssocItem::MacroCall(_) => None,
            })
    }

    fn import(&mut self, item: &ast::Use) {
        let mut pending: Vec<(Vec<String>, ast::UseTree)> = item
            .use_tree()
            .map(|tree| (Vec::new(), tree))
            .into_iter()
            .collect();
        while let Some((prefix, tree)) = pending.pop() {
            let mut path = prefix;
            if let Some(written) = tree.path() {
                let names = written.segments();
                path.extend(names.map(|segment| segment.syntax().text().to_string()));
            }
            if let Some(list) = tree.use_tree_list() {
                pending.extend(list.use_trees().map(|inner| (path.clone(), inner)));
            } else if tree.star_token().is_some() {
                self.imports.push(Import { alias: None, path });
            } else {
                let renamed = tree
                    .rename()
                    .and_then(|rename| rename.name())
                    .map(|name| name.text().to_string());
                let alias = renamed.or_else(|| path.last().cloned());
                self.imports.push(Import { alias, path });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cyclops_domain::facts::DecisionKind;

    fn facts(text: &str) -> FileFacts {
        extract(&RelativePath::new("src/lib.rs"), text)
    }

    fn symbol<'a>(facts: &'a FileFacts, name: &str) -> &'a SymbolFact {
        facts
            .symbols
            .iter()
            .find(|symbol| symbol.name == name)
            .expect("the symbol exists")
    }

    #[test]
    fn a_test_folder_makes_a_whole_file_test_code() {
        for path in ["tests/a.rs", "benches/a.rs", "crates/x/tests/a.rs"] {
            assert_eq!(
                file_role(&RelativePath::new(path)),
                CodeRole::Test,
                "{path}"
            );
        }
        for path in ["src/a.rs", "src/tests/a.rs", "tests.rs"] {
            assert_eq!(
                file_role(&RelativePath::new(path)),
                CodeRole::Production,
                "{path}"
            );
        }
    }

    #[test]
    fn test_lines_cover_the_test_module_with_its_attribute() {
        let facts = facts("fn f() {}\n\n#[cfg(test)]\nmod tests {\n    // c\n    fn g() {}\n}\n");
        let test_lines = LineCounts {
            code: 4,
            comment: 1,
            blank: 0,
        };
        assert_eq!(facts.test_lines, test_lines);
        assert_eq!(facts.lines.code, 5);
    }

    #[test]
    fn a_cfg_that_excludes_tests_is_not_test_code() {
        let facts = facts("#[cfg(not(test))]\nfn f() {}\n#[cfg(all(test, unix))]\nfn g() {}\n");
        assert_eq!(symbol(&facts, "f").role, CodeRole::Production);
        assert_eq!(symbol(&facts, "g").role, CodeRole::Test);
    }

    #[test]
    fn another_test_framework_is_recognized() {
        let facts = facts("#[tokio::test]\nasync fn f() {}\n");
        assert_eq!(symbol(&facts, "f").role, CodeRole::Test);
    }

    #[test]
    fn an_impl_records_the_type_as_written_without_generics() {
        let facts = facts("impl<T> a::B<T> { fn x(&self) {} }\nimpl T for &C { fn y(&self) {} }");
        let path = |parts: &[&str]| parts.iter().map(ToString::to_string).collect::<Vec<_>>();
        assert_eq!(symbol(&facts, "x").owner, Some(path(&["a", "B"])));
        assert_eq!(symbol(&facts, "y").owner, Some(path(&["C"])));
    }

    #[test]
    fn imports_are_flattened_and_keep_their_aliases() {
        let facts = facts("use crate::{a::B, c::{D as E, f::*}};");
        let mut imports: Vec<(Option<String>, String)> = facts
            .imports
            .iter()
            .map(|import| (import.alias.clone(), import.path.join("::")))
            .collect();
        imports.sort();
        assert_eq!(
            imports,
            [
                (None, "crate::c::f".to_owned()),
                (Some("B".to_owned()), "crate::a::B".to_owned()),
                (Some("E".to_owned()), "crate::c::D".to_owned()),
            ]
        );
    }

    #[test]
    fn a_declared_method_has_no_body() {
        let facts = facts("trait T { fn x(&self); }");
        assert_eq!(symbol(&facts, "x").body, None);
    }

    #[test]
    fn a_function_length_starts_at_its_signature() {
        let facts = facts("/// Doc.\n#[inline]\nfn f() {\n}\n");
        let f = symbol(&facts, "f");
        assert_eq!(f.span.start_line, 3);
        assert_eq!(f.code_lines, 2);
    }

    #[test]
    fn a_file_with_a_syntax_error_keeps_what_comes_before_it() {
        let facts = facts("fn kept() {}\nfn broken( {");
        assert!(facts.syntax_error);
        assert_eq!(symbol(&facts, "kept").kind, SymbolKind::Function);
    }

    #[test]
    fn a_decision_records_its_line() {
        let facts = facts("fn f(a: bool) {\n    if a {\n    } else {\n    }\n}\n");
        let decisions = &symbol(&facts, "f").body.as_ref().expect("a body").decisions;
        let kinds: Vec<_> = decisions
            .iter()
            .map(|point| (point.kind, point.line))
            .collect();
        assert_eq!(kinds, [(DecisionKind::If, 2), (DecisionKind::Else, 3)]);
    }

    #[test]
    fn code_too_deep_for_the_parser_is_a_syntax_error_and_not_a_crash() {
        let depth = 20_000;
        let text = format!(
            "fn f() {{ let a = {}1{}; }}\n\n",
            "(".repeat(depth),
            ")".repeat(depth)
        );
        let facts = facts(&text);
        assert!(facts.syntax_error);
        assert_eq!(facts.symbols, []);
        assert_eq!(facts.lines.code, 1);
        assert_eq!(facts.lines.blank, 1);
    }

    #[test]
    fn deep_but_reasonable_code_is_parsed() {
        let depth = MAX_BRACKET_DEPTH - 1;
        let text = format!("const C: u8 = {}1{};", "(".repeat(depth), ")".repeat(depth));
        assert!(!facts(&text).syntax_error);
    }

    #[test]
    fn closing_too_many_brackets_does_not_hide_deep_code() {
        let text = format!("}}}}}}{}", "(".repeat(MAX_BRACKET_DEPTH + 1));
        assert!(is_too_deep(&text));
    }
}

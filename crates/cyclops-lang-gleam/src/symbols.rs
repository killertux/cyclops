//! Walks a parsed Gleam file: finds its symbols, and describes what makes each function
//! complex. Gleam has no classes and no methods. A custom type with one constructor is a
//! struct whose labelled arguments are its fields, and one with several is an enum.

use crate::tree::{child_of_kind, each_node, end, named_children, start, text};
use cyclops_domain::facts::{
    BodyFacts, DecisionKind, DecisionPoint, Deepest, Import, MemberFacts, Reference, ReferenceKind,
    SymbolFact,
};
use cyclops_domain::lines::LineMap;
use cyclops_domain::symbol::{CodeRole, Span, SymbolKind, Visibility};
use tree_sitter::Node;

/// The function being walked.
#[derive(Default)]
struct Frame {
    name: String,
    /// The structures around the walk, from the outside in.
    trail: Vec<DecisionKind>,
    /// The deepest structure so far: its depth, its line, and what leads there.
    deepest: Option<(u32, u32, Vec<DecisionKind>)>,
    decisions: Vec<DecisionPoint>,
    max_nesting: u32,
    /// How deep the walk is for cognitive complexity, which also counts closures.
    nesting: u32,
    /// How many control structures are around the walk.
    depth: u32,
    recursive: bool,
}

pub(crate) struct Extractor<'l, 't> {
    lines: &'l LineMap,
    text: &'t str,
    role: CodeRole,
    symbols: Vec<SymbolFact>,
    imports: Vec<Import>,
    /// The symbol being filled.
    parent: Option<usize>,
    frame: Option<Frame>,
}

fn visibility(node: Node<'_>) -> Visibility {
    if child_of_kind(node, "visibility_modifier").is_some() {
        Visibility::Public
    } else {
        Visibility::Private
    }
}

/// The logical operator of a binary expression, if it is one.
fn logical_operator<'t>(node: Node<'_>, source: &'t str) -> Option<(u32, &'t str)> {
    if node.kind() != "binary_expression" {
        return None;
    }
    let operator = operator(node)?;
    let written = text(operator, source);
    matches!(written, "&&" | "||").then(|| (start(operator), written))
}

/// The operator of a binary expression: the one child the grammar does not name.
fn operator(node: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = node.walk();
    node.children(&mut cursor).find(|child| !child.is_named())
}

impl<'l, 't> Extractor<'l, 't> {
    pub(crate) fn new(lines: &'l LineMap, text: &'t str, role: CodeRole) -> Self {
        Self {
            lines,
            text,
            role,
            symbols: Vec::new(),
            imports: Vec::new(),
            parent: None,
            frame: None,
        }
    }

    pub(crate) fn into_parts(self) -> (Vec<SymbolFact>, Vec<Import>) {
        (self.symbols, self.imports)
    }

    fn text_of(&self, node: Node<'_>) -> &'t str {
        text(node, self.text)
    }

    fn field_text(&self, node: Node<'_>, field: &str) -> Option<&'t str> {
        node.child_by_field_name(field)
            .map(|child| self.text_of(child))
    }

    pub(crate) fn file(&mut self, root: Node<'_>) {
        for node in named_children(root) {
            match node.kind() {
                "import" => self.import(node),
                "type_definition" => self.type_definition(node),
                "type_alias" => {
                    if let Some(name) = self.type_name(node) {
                        self.plain(name, SymbolKind::Alias, visibility(node), node);
                    }
                }
                "constant" => {
                    if let Some(name) = self.field_text(node, "name") {
                        self.plain(name, SymbolKind::Constant, visibility(node), node);
                    }
                }
                "function" => self.function(node),
                _ => {}
            }
        }
    }

    /// `import shop/billing.{type Invoice, total as sum}` brings in the module, as
    /// `billing`, and each name it lists.
    fn import(&mut self, node: Node<'_>) {
        let Some(module) = self.field_text(node, "module") else {
            return;
        };
        let alias = self
            .field_text(node, "alias")
            .unwrap_or_else(|| module.rsplit('/').next().unwrap_or(module));
        self.imports.push(Import {
            alias: Some(alias.to_owned()),
            path: Vec::new(),
            from: Some(module.to_owned()),
            scope: None,
        });
        let Some(list) = node.child_by_field_name("imports") else {
            return;
        };
        for item in named_children(list) {
            let Some(name) = self.field_text(item, "name") else {
                continue;
            };
            let alias = self.field_text(item, "alias").unwrap_or(name);
            self.imports.push(Import {
                alias: Some(alias.to_owned()),
                path: vec![name.to_owned()],
                from: Some(module.to_owned()),
                scope: None,
            });
        }
    }

    fn type_name(&self, node: Node<'_>) -> Option<&'t str> {
        child_of_kind(node, "type_name").and_then(|name| self.field_text(name, "name"))
    }

    fn type_definition(&mut self, node: Node<'_>) {
        let Some(name) = self.type_name(node) else {
            return;
        };
        let constructors: Vec<Node<'_>> = child_of_kind(node, "data_constructors")
            .map(named_children)
            .unwrap_or_default()
            .into_iter()
            .filter(|child| child.kind() == "data_constructor")
            .collect();
        let visibility = visibility(node);
        if constructors.len() > 1 {
            self.plain(name, SymbolKind::Enum, visibility, node);
            return;
        }
        let index = self.plain(name, SymbolKind::Struct, visibility, node);
        let opaque = child_of_kind(node, "opacity_modifier").is_some();
        let field_visibility = if opaque {
            Visibility::Private
        } else {
            visibility
        };
        let arguments = constructors
            .first()
            .and_then(|constructor| constructor.child_by_field_name("arguments"))
            .map(named_children)
            .unwrap_or_default();
        let outer = self.parent.replace(index);
        for argument in arguments {
            let Some(label) = self.field_text(argument, "label") else {
                continue;
            };
            let field = self.plain(label, SymbolKind::Field, field_visibility, argument);
            let references = argument
                .child_by_field_name("value")
                .map(|value| self.type_references(value))
                .unwrap_or_default();
            if let Some(symbol) = self.symbols.get_mut(field) {
                symbol.references = references;
            }
        }
        self.parent = outer;
    }

    /// The types written in a type, such as `Item` and `List` in `List(cat.Item)`.
    fn type_references(&self, node: Node<'_>) -> Vec<Reference> {
        let mut found = Vec::new();
        each_node(node, |inner| {
            if inner.kind() != "type" {
                return;
            }
            let Some(name) = inner.child_by_field_name("name") else {
                return;
            };
            let path = match name.kind() {
                "type_identifier" => vec![self.text_of(name).to_owned()],
                "remote_type_identifier" => [
                    self.field_text(name, "module"),
                    self.field_text(name, "name"),
                ]
                .into_iter()
                .map(|part| part.map(ToOwned::to_owned))
                .collect::<Option<Vec<String>>>()
                .unwrap_or_default(),
                _ => Vec::new(),
            };
            if !path.is_empty() {
                found.push(Reference {
                    kind: ReferenceKind::Type,
                    path,
                });
            }
        });
        found
    }

    /// Adds a function, and walks its body. A function written elsewhere, with
    /// `@external`, has no body here.
    fn function(&mut self, node: Node<'_>) {
        let Some(name) = self.field_text(node, "name") else {
            return;
        };
        let index = self.plain(name, SymbolKind::Function, visibility(node), node);
        let parameters: Vec<Node<'_>> = node
            .child_by_field_name("parameters")
            .map(named_children)
            .unwrap_or_default()
            .into_iter()
            .filter(|parameter| parameter.kind() == "function_parameter")
            .collect();
        let mut references: Vec<Reference> = parameters
            .iter()
            .filter_map(|parameter| parameter.child_by_field_name("type"))
            .flat_map(|annotation| self.type_references(annotation))
            .collect();
        if let Some(returns) = node.child_by_field_name("return_type") {
            references.extend(self.type_references(returns));
        }
        if let Some(symbol) = self.symbols.get_mut(index) {
            symbol.references = references;
        }
        let Some(body) = node.child_by_field_name("body") else {
            return;
        };
        let outer_frame = self.frame.replace(Frame {
            name: name.to_owned(),
            ..Frame::default()
        });
        let outer_parent = self.parent.replace(index);
        self.walk(body);
        self.parent = outer_parent;
        let finished = std::mem::replace(&mut self.frame, outer_frame);
        if let (Some(mut frame), Some(symbol)) = (finished, self.symbols.get_mut(index)) {
            frame.decisions.sort_by_key(|point| point.line);
            symbol.body = Some(BodyFacts {
                decisions: frame.decisions,
                parameters: u32::try_from(parameters.len()).unwrap_or(u32::MAX),
                max_nesting: frame.max_nesting,
                fields_used: Vec::new(),
                methods_called: Vec::new(),
                deepest: frame.deepest.map(|(_, line, path)| Deepest { line, path }),
            });
        }
    }

    fn fact(
        &self,
        name: &str,
        kind: SymbolKind,
        visibility: Visibility,
        node: Node<'_>,
    ) -> SymbolFact {
        let start_line = self.lines.line_of(start(node));
        let end_line = self
            .lines
            .line_of(end(node).saturating_sub(1).max(start(node)));
        SymbolFact {
            name: name.to_owned(),
            parent: self.parent,
            owner: None,
            kind,
            role: self.role,
            visibility,
            span: Span {
                start_line,
                end_line,
            },
            code_lines: self.lines.counts_between(start_line, end_line).code,
            body: None,
            member: MemberFacts::default(),
            references: Vec::new(),
        }
    }

    fn plain(
        &mut self,
        name: &str,
        kind: SymbolKind,
        visibility: Visibility,
        node: Node<'_>,
    ) -> usize {
        let fact = self.fact(name, kind, visibility, node);
        self.symbols.push(fact);
        self.symbols.len() - 1
    }

    /// Records something the function being walked mentions.
    fn reference(&mut self, kind: ReferenceKind, path: Vec<String>) {
        if self.frame.is_none() || path.is_empty() {
            return;
        }
        if let Some(symbol) = self.parent.and_then(|index| self.symbols.get_mut(index)) {
            symbol.references.push(Reference { kind, path });
        }
    }

    fn walk_children(&mut self, node: Node<'_>) {
        for child in named_children(node) {
            self.walk(child);
        }
    }

    /// Walks the body of a function. Everything that is not a structure only adds what
    /// it mentions.
    fn walk(&mut self, node: Node<'_>) {
        match node.kind() {
            "case" => self.case(node),
            "binary_expression" => self.binary(node),
            "function_call" => self.call(node),
            "field_access" => self.field_access(node),
            "record" | "record_pattern" => {
                if let Some(name) = node.child_by_field_name("name") {
                    self.constructor(name);
                }
                self.walk_children(node);
            }
            "record_update" => {
                if let Some(name) = node.child_by_field_name("constructor") {
                    self.constructor(name);
                }
                self.walk_children(node);
            }
            "anonymous_function" => self.inside_closure(|walker| walker.walk_children(node)),
            "type" => {
                for reference in self.type_references(node) {
                    self.reference(reference.kind, reference.path);
                }
            }
            _ => self.walk_children(node),
        }
    }

    /// Building or matching a record mentions its type, which in Gleam usually shares
    /// the constructor's name.
    fn constructor(&mut self, name: Node<'_>) {
        let path = match name.kind() {
            "constructor_name" => vec![self.text_of(name).to_owned()],
            "remote_constructor_name" => [
                self.field_text(name, "module"),
                self.field_text(name, "name"),
            ]
            .into_iter()
            .map(|part| part.map(ToOwned::to_owned))
            .collect::<Option<Vec<String>>>()
            .unwrap_or_default(),
            _ => Vec::new(),
        };
        self.reference(ReferenceKind::Type, path);
    }

    /// `module.name` calls or passes a function of another module. On a record, it reads
    /// a field, and linking finds no module of that name.
    fn field_access(&mut self, node: Node<'_>) {
        let record = node.child_by_field_name("record");
        let field = self.field_text(node, "field");
        match (record, field) {
            (Some(record), Some(field)) if record.kind() == "identifier" => {
                let module = self.text_of(record).to_owned();
                self.reference(ReferenceKind::Call, vec![module, field.to_owned()]);
            }
            (Some(record), _) => self.walk(record),
            _ => {}
        }
    }

    /// A function called by its own name, or through its module.
    fn callee(&mut self, function: Node<'_>) {
        if function.kind() == "identifier" {
            let name = self.text_of(function);
            if self.frame.as_ref().is_some_and(|frame| frame.name == name) {
                self.recursion(start(function));
            }
            self.reference(ReferenceKind::Call, vec![name.to_owned()]);
        } else {
            self.walk(function);
        }
    }

    fn call(&mut self, node: Node<'_>) {
        if let Some(function) = node.child_by_field_name("function") {
            self.callee(function);
        }
        if let Some(arguments) = node.child_by_field_name("arguments") {
            self.walk(arguments);
        }
    }

    fn binary(&mut self, node: Node<'_>) {
        if logical_operator(node, self.text).is_some() {
            self.logical(node);
            return;
        }
        let is_pipe = operator(node).is_some_and(|operator| self.text_of(operator) == "|>");
        if let Some(left) = node.child_by_field_name("left") {
            self.walk(left);
        }
        if let Some(right) = node.child_by_field_name("right") {
            if is_pipe {
                self.callee(right);
            } else {
                self.walk(right);
            }
        }
    }

    /// Every logical operator is a branch, and each run of the same one adds a point.
    fn logical(&mut self, top: Node<'_>) {
        let mut operators = Vec::new();
        let mut operands = Vec::new();
        let mut pending = vec![top];
        while let Some(node) = pending.pop() {
            match logical_operator(node, self.text) {
                Some(found) => {
                    operators.push(found);
                    for side in ["right", "left"] {
                        if let Some(child) = node.child_by_field_name(side) {
                            pending.push(child);
                        }
                    }
                }
                None => operands.push(node),
            }
        }
        operators.sort_by_key(|(offset, _)| *offset);
        let mut previous = None;
        for (offset, operator) in operators {
            self.point(DecisionKind::LogicalOperator, offset);
            if previous != Some(operator) {
                self.point(DecisionKind::LogicalRun, offset);
            }
            previous = Some(operator);
        }
        for operand in operands {
            self.walk(operand);
        }
    }

    /// A `case` is a `match`: each clause after the first is one more way out, and so is
    /// each guard.
    fn case(&mut self, node: Node<'_>) {
        let at = start(node);
        self.point(DecisionKind::Match, at);
        if let Some(subjects) = node.child_by_field_name("subjects") {
            self.walk(subjects);
        }
        let clauses: Vec<Node<'_>> = node
            .child_by_field_name("clauses")
            .map(named_children)
            .unwrap_or_default()
            .into_iter()
            .filter(|clause| clause.kind() == "case_clause")
            .collect();
        for (index, clause) in clauses.into_iter().enumerate() {
            if index > 0 {
                self.point(DecisionKind::MatchArm, start(clause));
            }
            if let Some(patterns) = clause.child_by_field_name("patterns") {
                self.walk(patterns);
            }
            if let Some(guard) = clause.child_by_field_name("guard") {
                self.point(DecisionKind::Guard, start(guard));
                self.walk(guard);
            }
            if let Some(value) = clause.child_by_field_name("value") {
                self.deeper(DecisionKind::Match, at, |walker| walker.walk(value));
            }
        }
    }

    fn point(&mut self, kind: DecisionKind, offset: u32) {
        let line = self.lines.line_of(offset);
        if let Some(frame) = &mut self.frame {
            frame.decisions.push(DecisionPoint {
                kind,
                line,
                nesting: frame.nesting,
            });
        }
    }

    /// Walks something one control structure deeper, and keeps the deepest structure,
    /// the first in the code when there are several.
    fn deeper(&mut self, kind: DecisionKind, offset: u32, walk: impl FnOnce(&mut Self)) {
        let line = self.lines.line_of(offset);
        if let Some(frame) = &mut self.frame {
            frame.nesting += 1;
            frame.depth += 1;
            frame.max_nesting = frame.max_nesting.max(frame.depth);
            frame.trail.push(kind);
            let deeper_than_known = frame.deepest.as_ref().is_none_or(|(known, known_line, _)| {
                frame.depth > *known || (frame.depth == *known && line < *known_line)
            });
            if deeper_than_known {
                frame.deepest = Some((frame.depth, line, frame.trail.clone()));
            }
        }
        walk(self);
        if let Some(frame) = &mut self.frame {
            frame.nesting -= 1;
            frame.depth -= 1;
            frame.trail.pop();
        }
    }

    /// An anonymous function is one level deeper to read, and adds no control structure.
    fn inside_closure(&mut self, walk: impl FnOnce(&mut Self)) {
        if let Some(frame) = &mut self.frame {
            frame.nesting += 1;
        }
        walk(self);
        if let Some(frame) = &mut self.frame {
            frame.nesting -= 1;
        }
    }

    /// A function is only charged once for calling itself.
    fn recursion(&mut self, offset: u32) {
        let charged = self
            .frame
            .as_mut()
            .is_some_and(|frame| std::mem::replace(&mut frame.recursive, true));
        if !charged {
            self.point(DecisionKind::Recursion, offset);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::extract;
    use cyclops_domain::facts::FileFacts;
    use cyclops_domain::path::RelativePath;

    fn facts(text: &str) -> FileFacts {
        extract(&RelativePath::new("src/a.gleam"), text)
    }

    fn symbol<'f>(facts: &'f FileFacts, name: &str) -> &'f SymbolFact {
        facts
            .symbols
            .iter()
            .find(|symbol| symbol.name == name)
            .expect("the symbol")
    }

    fn kinds(text: &str, name: &str) -> Vec<DecisionKind> {
        let facts = facts(text);
        symbol(&facts, name)
            .body
            .as_ref()
            .expect("a body")
            .decisions
            .iter()
            .map(|point| point.kind)
            .collect()
    }

    #[test]
    fn a_record_type_is_a_struct_with_its_labelled_fields() {
        let facts = facts("pub type S { S(a: Int, Int, b: List(Item)) }");
        assert_eq!(symbol(&facts, "S").kind, SymbolKind::Struct);
        let names: Vec<&str> = facts.symbols.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["S", "a", "b"]);
        let b = symbol(&facts, "b");
        assert_eq!(b.parent, Some(0));
        let mentioned: Vec<&[String]> = b.references.iter().map(|r| r.path.as_slice()).collect();
        assert_eq!(
            mentioned,
            [vec!["List".to_owned()], vec!["Item".to_owned()]]
        );
    }

    #[test]
    fn a_type_of_several_constructors_is_an_enum_without_fields() {
        let facts = facts("type E { A(x: Int) B }");
        assert_eq!(symbol(&facts, "E").kind, SymbolKind::Enum);
        assert_eq!(facts.symbols.len(), 1);
    }

    #[test]
    fn an_opaque_type_hides_its_fields() {
        let facts = facts("pub opaque type S { S(a: Int) }");
        assert_eq!(symbol(&facts, "S").visibility, Visibility::Public);
        assert_eq!(symbol(&facts, "a").visibility, Visibility::Private);
    }

    #[test]
    fn imports_bring_in_the_module_and_each_listed_name() {
        let facts =
            facts("import shop/billing.{type Invoice, total as sum}\nimport gleam/list as l\n");
        let imports: Vec<(Option<&str>, Vec<&str>, Option<&str>)> = facts
            .imports
            .iter()
            .map(|import| {
                (
                    import.alias.as_deref(),
                    import.path.iter().map(String::as_str).collect(),
                    import.from.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            imports,
            [
                (Some("billing"), vec![], Some("shop/billing")),
                (Some("Invoice"), vec!["Invoice"], Some("shop/billing")),
                (Some("sum"), vec!["total"], Some("shop/billing")),
                (Some("l"), vec![], Some("gleam/list")),
            ]
        );
    }

    #[test]
    fn calls_name_their_function_or_its_module() {
        let facts = facts("fn f(x) {\n  g(x)\n  billing.total(x)\n  x |> billing.is_free\n}\n");
        let calls: Vec<String> = symbol(&facts, "f")
            .references
            .iter()
            .filter(|reference| reference.kind == ReferenceKind::Call)
            .map(|reference| reference.path.join("."))
            .collect();
        assert_eq!(calls, ["g", "billing.total", "billing.is_free"]);
    }

    #[test]
    fn a_case_counts_its_clauses_and_guards() {
        assert_eq!(
            kinds("fn f(a) { case a { x if x > 1 -> 1 _ -> 0 } }", "f"),
            [
                DecisionKind::Match,
                DecisionKind::Guard,
                DecisionKind::MatchArm
            ]
        );
    }

    #[test]
    fn a_change_of_logical_operator_starts_a_new_run() {
        assert_eq!(
            kinds("fn f(a, b, c) { a && b || c }", "f"),
            [
                DecisionKind::LogicalOperator,
                DecisionKind::LogicalRun,
                DecisionKind::LogicalOperator,
                DecisionKind::LogicalRun
            ]
        );
    }

    #[test]
    fn calling_itself_is_recursion_once() {
        assert_eq!(
            kinds("fn f(n) { f(n) + f(n) }", "f"),
            [DecisionKind::Recursion]
        );
    }

    #[test]
    fn an_external_function_has_no_body() {
        let facts = facts("@external(erlang, \"m\", \"f\")\npub fn f() -> Int\n");
        let f = symbol(&facts, "f");
        assert_eq!(f.kind, SymbolKind::Function);
        assert_eq!(f.body, None);
    }
}

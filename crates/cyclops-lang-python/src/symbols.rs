//! Walks a parsed Python file: finds its symbols, and describes what makes each function
//! complex. Everything that is not a symbol counts towards the function around it.
//!
//! A class is a class, unless it is a protocol or an abstract base, which is an
//! interface, or an enum. A name assigned at the top of a module is a constant, since
//! Python has no constants of its own. A field is a name declared in a class body, or
//! assigned on `self` in any of its methods.

use crate::tree::{end, named_children, start, text};
use cyclops_domain::facts::{
    BodyFacts, DecisionKind, DecisionPoint, Deepest, Import, MemberFacts, Reference, ReferenceKind,
    SymbolFact,
};
use cyclops_domain::lines::LineMap;
use cyclops_domain::symbol::{CodeRole, Span, SymbolKind, Visibility};
use std::collections::HashSet;
use tree_sitter::Node;

/// Bases that make a class an interface.
const INTERFACES: [&str; 2] = ["Protocol", "ABC"];

/// Bases that make a class an enum.
const ENUMS: [&str; 6] = ["Enum", "IntEnum", "StrEnum", "Flag", "IntFlag", "ReprEnum"];

/// Bases that say what a class is rather than what it extends.
const NOT_EXTENDED: [&str; 3] = ["object", "Generic", "TypedDict"];

/// The kinds of parameter node that are not parameters, such as `/` and `*`.
const SEPARATORS: [&str; 2] = ["positional_separator", "keyword_separator"];

/// The comprehensions, whose `for` and `if` read on one line.
const COMPREHENSIONS: [&str; 4] = [
    "list_comprehension",
    "set_comprehension",
    "dictionary_comprehension",
    "generator_expression",
];

/// The function being walked.
#[derive(Default)]
struct Frame {
    name: String,
    is_method: bool,
    /// What the method calls its instance, usually `self`.
    receiver: Option<String>,
    /// The class of a method.
    owner: Option<usize>,
    fields_used: Vec<String>,
    methods_called: Vec<String>,
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

/// A field a method creates by assigning it on `self`.
struct Assigned {
    class: usize,
    name: String,
    span: (u32, u32),
    references: Vec<Reference>,
}

pub(crate) struct Extractor<'l, 't> {
    lines: &'l LineMap,
    text: &'t str,
    role: CodeRole,
    symbols: Vec<SymbolFact>,
    imports: Vec<Import>,
    /// The names the file's imports bind, which calls such as `billing.total()` go
    /// through.
    imported: HashSet<String>,
    assigned: Vec<Assigned>,
    /// The symbol being filled.
    parent: Option<usize>,
    frame: Option<Frame>,
}

/// A leading `_` makes a name private, but a name like `__init__`, which Python itself
/// calls, is public.
fn visibility(name: &str) -> Visibility {
    let special = name.len() > 4 && name.starts_with("__") && name.ends_with("__");
    if name.starts_with('_') && !special {
        Visibility::Private
    } else {
        Visibility::Public
    }
}

fn last_name(path: &[String]) -> &str {
    path.last().map_or("", String::as_str)
}

impl<'l, 't> Extractor<'l, 't> {
    pub(crate) fn new(lines: &'l LineMap, text: &'t str, role: CodeRole) -> Self {
        Self {
            lines,
            text,
            role,
            symbols: Vec::new(),
            imports: Vec::new(),
            imported: HashSet::new(),
            assigned: Vec::new(),
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

    /// A dotted name as its parts, such as `abc` and `ABC` in `abc.ABC`.
    fn path(&self, node: Node<'_>) -> Option<Vec<String>> {
        match node.kind() {
            "identifier" | "attribute" | "dotted_name" => Some(
                self.text_of(node)
                    .split('.')
                    .map(|part| part.trim().to_owned())
                    .collect(),
            ),
            _ => None,
        }
    }

    pub(crate) fn file(&mut self, root: Node<'_>) {
        self.find_imports(root);
        self.statements(root);
    }

    /// Every import of the file, wherever it is, since a function's import is a
    /// dependency of its module too.
    fn find_imports(&mut self, root: Node<'_>) {
        let mut pending = vec![root];
        while let Some(node) = pending.pop() {
            match node.kind() {
                "import_statement" => self.import(node),
                "import_from_statement" => self.import_from(node),
                _ => pending.extend(named_children(node).into_iter().rev()),
            }
        }
    }

    fn add_import(&mut self, alias: Option<&str>, path: Vec<String>, from: &str) {
        if let Some(alias) = alias {
            self.imported.insert(alias.to_owned());
        }
        self.imports.push(Import {
            alias: alias.map(ToOwned::to_owned),
            path,
            from: Some(from.to_owned()),
            scope: None,
        });
    }

    /// `import shop.billing as b`.
    fn import(&mut self, node: Node<'_>) {
        let mut cursor = node.walk();
        let names: Vec<Node<'_>> = node.children_by_field_name("name", &mut cursor).collect();
        for name in names {
            let (module, alias) = match name.kind() {
                "aliased_import" => (
                    self.field_text(name, "name"),
                    self.field_text(name, "alias"),
                ),
                _ => (Some(self.text_of(name)), None),
            };
            if let Some(module) = module {
                self.add_import(Some(alias.unwrap_or(module)), Vec::new(), module);
            }
        }
    }

    /// `from .catalog import Item as I`, and `from shop import *`.
    fn import_from(&mut self, node: Node<'_>) {
        let Some(module) = self.field_text(node, "module_name") else {
            return;
        };
        let mut cursor = node.walk();
        let names: Vec<Node<'_>> = node.children_by_field_name("name", &mut cursor).collect();
        let everything = names.is_empty()
            || named_children(node)
                .iter()
                .any(|child| child.kind() == "wildcard_import");
        if everything {
            self.add_import(None, Vec::new(), module);
            return;
        }
        for name in names {
            let (original, alias) = match name.kind() {
                "aliased_import" => (
                    self.field_text(name, "name"),
                    self.field_text(name, "alias"),
                ),
                _ => (Some(self.text_of(name)), None),
            };
            if let Some(original) = original {
                self.add_import(
                    Some(alias.unwrap_or(original)),
                    vec![original.to_owned()],
                    module,
                );
            }
        }
    }

    /// The statements of a module or a class body: what they define. Blocks such as
    /// `if TYPE_CHECKING:` or `try:` define things too.
    fn statements(&mut self, body: Node<'_>) {
        for node in named_children(body) {
            match node.kind() {
                "function_definition" => self.function(node, &[]),
                "class_definition" => self.class(node),
                "decorated_definition" => self.decorated(node),
                "expression_statement" => {
                    for child in named_children(node) {
                        if child.kind() == "assignment" {
                            self.definition(child);
                        }
                    }
                }
                "type_alias_statement" => {
                    let name = node
                        .child_by_field_name("left")
                        .map(|left| self.text_of(left));
                    if let Some(name) = name {
                        self.plain(name, SymbolKind::Alias, node);
                    }
                }
                "if_statement" | "try_statement" | "with_statement" | "elif_clause"
                | "else_clause" | "except_clause" | "finally_clause" | "block" => {
                    self.statements(node);
                }
                _ => {}
            }
        }
    }

    fn decorated(&mut self, node: Node<'_>) {
        let decorators: Vec<String> = named_children(node)
            .into_iter()
            .filter(|child| child.kind() == "decorator")
            .map(|decorator| {
                let written = self.text_of(decorator).trim_start_matches('@');
                let name = written.split('(').next().unwrap_or(written);
                name.rsplit('.').next().unwrap_or(name).trim().to_owned()
            })
            .collect();
        match node.child_by_field_name("definition") {
            Some(definition) if definition.kind() == "function_definition" => {
                self.function(definition, &decorators);
            }
            Some(definition) if definition.kind() == "class_definition" => self.class(definition),
            _ => {}
        }
    }

    fn inside_class(&self) -> bool {
        self.parent
            .and_then(|index| self.symbols.get(index))
            .is_some_and(|parent| parent.kind.is_type())
    }

    /// `NAME = value` at the top of a module is a constant, `A: TypeAlias = int` an
    /// alias, `f = lambda: ...` a function, and in a class body each is a field.
    fn definition(&mut self, assignment: Node<'_>) {
        let Some(left) = assignment.child_by_field_name("left") else {
            return;
        };
        if left.kind() != "identifier" {
            return;
        }
        let name = self.text_of(left);
        let annotation = assignment.child_by_field_name("type");
        let right = assignment.child_by_field_name("right");
        let is_alias = annotation.is_some_and(|annotation| {
            let written = self.text_of(annotation);
            written == "TypeAlias" || written.ends_with(".TypeAlias")
        });
        if is_alias {
            self.plain(name, SymbolKind::Alias, assignment);
            return;
        }
        let lambda_body = right
            .filter(|right| right.kind() == "lambda")
            .and_then(|lambda| lambda.child_by_field_name("body"));
        if let Some(body) = lambda_body {
            let kind = if self.inside_class() {
                SymbolKind::Method
            } else {
                SymbolKind::Function
            };
            self.callable(name, kind, assignment, body, &[]);
            return;
        }
        let kind = if self.inside_class() {
            SymbolKind::Field
        } else {
            SymbolKind::Constant
        };
        let known = self
            .symbols
            .iter()
            .any(|symbol| symbol.parent == self.parent && symbol.name == name);
        if known {
            return;
        }
        let index = self.plain(name, kind, assignment);
        let references = annotation
            .map(|annotation| self.type_references(annotation))
            .unwrap_or_default();
        if let Some(symbol) = self.symbols.get_mut(index) {
            symbol.references = references;
        }
    }

    fn class(&mut self, node: Node<'_>) {
        let Some(name) = self.field_text(node, "name") else {
            return;
        };
        let mut bases: Vec<Vec<String>> = Vec::new();
        let mut metaclass = None;
        if let Some(arguments) = node.child_by_field_name("superclasses") {
            for argument in named_children(arguments) {
                if argument.kind() == "keyword_argument" {
                    if self.field_text(argument, "name") == Some("metaclass") {
                        metaclass = argument
                            .child_by_field_name("value")
                            .and_then(|value| self.path(value));
                    }
                } else if let Some(path) = self.path(argument) {
                    bases.push(path);
                }
            }
        }
        let base_names: Vec<&str> = bases.iter().map(|base| last_name(base)).collect();
        let kind = if base_names.iter().any(|base| INTERFACES.contains(base))
            || metaclass
                .as_deref()
                .is_some_and(|meta| last_name(meta) == "ABCMeta")
        {
            SymbolKind::Interface
        } else if base_names.iter().any(|base| ENUMS.contains(base)) {
            SymbolKind::Enum
        } else {
            SymbolKind::Class
        };
        let index = self.plain(name, kind, node);
        let references = bases
            .into_iter()
            .filter(|base| {
                let last = last_name(base);
                !INTERFACES.contains(&last)
                    && !ENUMS.contains(&last)
                    && !NOT_EXTENDED.contains(&last)
            })
            .map(|path| Reference {
                kind: ReferenceKind::Inherits,
                path,
            })
            .collect();
        if let Some(symbol) = self.symbols.get_mut(index) {
            symbol.references = references;
        }
        let outer_parent = self.parent.replace(index);
        let outer_frame = self.frame.take();
        if let Some(body) = node.child_by_field_name("body") {
            self.statements(body);
        }
        self.frame = outer_frame;
        let assigned: Vec<Assigned> = std::mem::take(&mut self.assigned)
            .into_iter()
            .filter_map(|field| {
                if field.class != index {
                    return Some(field);
                }
                let declared = self
                    .symbols
                    .iter()
                    .any(|symbol| symbol.parent == Some(index) && symbol.name == field.name);
                if !declared {
                    let at = self.push_span(&field.name, SymbolKind::Field, field.span);
                    if let Some(symbol) = self.symbols.get_mut(at) {
                        symbol.references = field.references;
                    }
                }
                None
            })
            .collect();
        self.assigned = assigned;
        self.parent = outer_parent;
    }

    fn function(&mut self, node: Node<'_>, decorators: &[String]) {
        let Some(name) = self.field_text(node, "name") else {
            return;
        };
        let kind = if self.inside_class() {
            SymbolKind::Method
        } else {
            SymbolKind::Function
        };
        let Some(body) = node.child_by_field_name("body") else {
            return;
        };
        self.callable(name, kind, node, body, decorators);
    }

    /// Adds a function, a method or a lambda held in a name, and walks its body.
    fn callable(
        &mut self,
        name: &str,
        kind: SymbolKind,
        node: Node<'_>,
        body: Node<'_>,
        decorators: &[String],
    ) {
        let owner = (kind == SymbolKind::Method)
            .then_some(self.parent)
            .flatten();
        let is_static = decorators
            .iter()
            .any(|decorator| decorator == "staticmethod");
        let is_class_method = decorators
            .iter()
            .any(|decorator| decorator == "classmethod");
        let constructor = matches!(name, "__init__" | "__new__");
        let index = self.plain(name, kind, node);
        let definition = if node.kind() == "assignment" {
            node.child_by_field_name("right")
        } else {
            Some(node)
        };
        let parameters: Vec<Node<'_>> = definition
            .and_then(|definition| definition.child_by_field_name("parameters"))
            .map(named_children)
            .unwrap_or_default()
            .into_iter()
            .filter(|parameter| !SEPARATORS.contains(&parameter.kind()))
            .collect();
        let takes_receiver = kind == SymbolKind::Method && !is_static;
        let receiver = takes_receiver
            .then(|| parameters.first())
            .flatten()
            .map(|first| self.parameter_name(*first).to_owned());
        let counted = parameters
            .len()
            .saturating_sub(usize::from(receiver.is_some()));
        let mut references: Vec<Reference> = parameters
            .iter()
            .filter_map(|parameter| parameter.child_by_field_name("type"))
            .flat_map(|annotation| self.type_references(annotation))
            .collect();
        if let Some(returns) =
            definition.and_then(|definition| definition.child_by_field_name("return_type"))
        {
            references.extend(self.type_references(returns));
        }
        if let Some(symbol) = self.symbols.get_mut(index) {
            symbol.references = references;
            symbol.member = MemberFacts {
                instance: takes_receiver && !is_class_method && !constructor,
                for_interface: false,
            };
        }
        let outer_frame = self.frame.replace(Frame {
            name: name.to_owned(),
            is_method: kind == SymbolKind::Method,
            receiver: receiver.filter(|_| !is_class_method),
            owner,
            ..Frame::default()
        });
        let outer_parent = self.parent.replace(index);
        for parameter in &parameters {
            if let Some(default) = parameter.child_by_field_name("value") {
                self.walk(default);
            }
        }
        self.walk(body);
        self.parent = outer_parent;
        let finished = std::mem::replace(&mut self.frame, outer_frame);
        if let (Some(mut frame), Some(symbol)) = (finished, self.symbols.get_mut(index)) {
            frame.decisions.sort_by_key(|point| point.line);
            frame.fields_used.sort();
            frame.fields_used.dedup();
            frame.methods_called.sort();
            frame.methods_called.dedup();
            symbol.body = Some(BodyFacts {
                decisions: frame.decisions,
                parameters: u32::try_from(counted).unwrap_or(u32::MAX),
                max_nesting: frame.max_nesting,
                fields_used: frame.fields_used,
                methods_called: frame.methods_called,
                deepest: frame.deepest.map(|(_, line, path)| Deepest { line, path }),
            });
        }
    }

    /// The name a parameter binds, such as `self` in `self` or `a` in `a: int = 1`.
    fn parameter_name(&self, parameter: Node<'_>) -> &'t str {
        let named = match parameter.kind() {
            "identifier" => Some(parameter),
            "default_parameter" | "typed_default_parameter" => {
                parameter.child_by_field_name("name")
            }
            _ => named_children(parameter)
                .into_iter()
                .find(|child| child.kind() == "identifier"),
        };
        named.map_or("", |name| self.text_of(name))
    }

    /// The types an annotation names, such as `list` and `Invoice` in
    /// `list[Invoice]`. A quoted name, written before its type exists, counts too.
    fn type_references(&self, node: Node<'_>) -> Vec<Reference> {
        let mut found = Vec::new();
        let mut pending = vec![node];
        while let Some(current) = pending.pop() {
            if let Some(path) = self.path(current) {
                found.push(path);
                continue;
            }
            if current.kind() == "string" {
                let written = self
                    .text_of(current)
                    .trim_matches(|c| c == '"' || c == '\'');
                let is_name = !written.is_empty()
                    && written
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_' || c == '.');
                if is_name {
                    found.push(written.split('.').map(ToOwned::to_owned).collect());
                }
                continue;
            }
            pending.extend(named_children(current).into_iter().rev());
        }
        found
            .into_iter()
            .map(|path| Reference {
                kind: ReferenceKind::Type,
                path,
            })
            .collect()
    }

    fn fact(&self, name: &str, kind: SymbolKind, (from, to): (u32, u32)) -> SymbolFact {
        let start_line = self.lines.line_of(from);
        let end_line = self.lines.line_of(to.saturating_sub(1).max(from));
        SymbolFact {
            name: name.to_owned(),
            parent: self.parent,
            owner: None,
            kind,
            role: self.role,
            visibility: visibility(name),
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

    fn plain(&mut self, name: &str, kind: SymbolKind, node: Node<'_>) -> usize {
        self.push_span(name, kind, (start(node), end(node)))
    }

    fn push_span(&mut self, name: &str, kind: SymbolKind, span: (u32, u32)) -> usize {
        let fact = self.fact(name, kind, span);
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

    fn walk_field(&mut self, node: Node<'_>, field: &str) {
        if let Some(child) = node.child_by_field_name(field) {
            self.walk(child);
        }
    }

    /// Walks the body of a function. Everything that is not a structure only adds what
    /// it mentions.
    fn walk(&mut self, node: Node<'_>) {
        match node.kind() {
            "function_definition" => self.function(node, &[]),
            "class_definition" => self.class(node),
            "decorated_definition" => self.decorated(node),
            "if_statement" => self.conditional(node),
            "for_statement" | "while_statement" => self.looping(node),
            "match_statement" => self.matching(node),
            "try_statement" => self.attempt(node),
            "conditional_expression" => self.ternary(node),
            "boolean_operator" => self.logical(node),
            "lambda" => self.inside_closure(|walker| walker.walk_children(node)),
            "call" => self.call(node),
            "attribute" => self.attribute(node),
            "assignment" | "augmented_assignment" => self.assignment(node),
            "type" => {
                for reference in self.type_references(node) {
                    self.reference(reference.kind, reference.path);
                }
            }
            kind if COMPREHENSIONS.contains(&kind) => self.comprehension(node),
            _ => self.walk_children(node),
        }
    }

    fn is_receiver(&self, node: Node<'_>) -> bool {
        node.kind() == "identifier"
            && self
                .frame
                .as_ref()
                .and_then(|frame| frame.receiver.as_deref())
                .is_some_and(|receiver| receiver == self.text_of(node))
    }

    /// `self.name` reads a field.
    fn attribute(&mut self, node: Node<'_>) {
        let object = node.child_by_field_name("object");
        let name = self.field_text(node, "attribute");
        match (object, name) {
            (Some(object), Some(name)) if self.is_receiver(object) => {
                if let Some(frame) = &mut self.frame {
                    frame.fields_used.push(name.to_owned());
                }
            }
            (Some(object), _) => self.walk(object),
            _ => {}
        }
    }

    /// `self.name = value` creates a field of the class.
    fn assignment(&mut self, node: Node<'_>) {
        let left = node.child_by_field_name("left");
        let on_receiver = left.filter(|left| {
            left.kind() == "attribute"
                && left
                    .child_by_field_name("object")
                    .is_some_and(|object| self.is_receiver(object))
        });
        let class = self.frame.as_ref().and_then(|frame| frame.owner);
        if let (Some(target), Some(class)) = (on_receiver, class)
            && let Some(name) = self.field_text(target, "attribute")
        {
            let references = node
                .child_by_field_name("type")
                .map(|annotation| self.type_references(annotation))
                .unwrap_or_default();
            self.assigned.push(Assigned {
                class,
                name: name.to_owned(),
                span: (start(node), end(node)),
                references,
            });
        }
        self.walk_children(node);
    }

    fn call(&mut self, node: Node<'_>) {
        if let Some(function) = node.child_by_field_name("function") {
            self.callee(function);
        }
        self.walk_field(node, "arguments");
    }

    /// What a call calls: a name, a method of `self`, a function of an imported module,
    /// or a method of something whose type is not known.
    fn callee(&mut self, function: Node<'_>) {
        let (is_method, frame_name) = self.frame.as_ref().map_or((false, String::new()), |frame| {
            (frame.is_method, frame.name.clone())
        });
        match function.kind() {
            "identifier" => {
                let name = self.text_of(function);
                if !is_method && name == frame_name {
                    self.recursion(start(function));
                }
                self.reference(ReferenceKind::Call, vec![name.to_owned()]);
            }
            "attribute" => {
                let object = function.child_by_field_name("object");
                let Some(name) = self.field_text(function, "attribute") else {
                    return;
                };
                match object {
                    Some(object) if self.is_receiver(object) => {
                        if is_method && name == frame_name {
                            self.recursion(start(function));
                        }
                        if let Some(frame) = &mut self.frame {
                            frame.methods_called.push(name.to_owned());
                        }
                        self.reference(ReferenceKind::SelfCall, vec![name.to_owned()]);
                    }
                    Some(object)
                        if object.kind() == "identifier"
                            && self.imported.contains(self.text_of(object)) =>
                    {
                        let module = self.text_of(object).to_owned();
                        self.reference(ReferenceKind::Call, vec![module, name.to_owned()]);
                    }
                    Some(object) => {
                        self.reference(ReferenceKind::MethodCall, vec![name.to_owned()]);
                        self.walk(object);
                    }
                    None => {}
                }
            }
            _ => self.walk(function),
        }
    }

    fn conditional(&mut self, node: Node<'_>) {
        let at = start(node);
        self.point(DecisionKind::If, at);
        self.walk_field(node, "condition");
        if let Some(consequence) = node.child_by_field_name("consequence") {
            self.deeper(DecisionKind::If, at, |walker| walker.walk(consequence));
        }
        let mut cursor = node.walk();
        let alternatives: Vec<Node<'_>> = node
            .children_by_field_name("alternative", &mut cursor)
            .collect();
        for alternative in alternatives {
            if alternative.kind() == "elif_clause" {
                self.point(DecisionKind::ElseIf, start(alternative));
                self.walk_field(alternative, "condition");
                if let Some(consequence) = alternative.child_by_field_name("consequence") {
                    self.deeper(DecisionKind::If, at, |walker| walker.walk(consequence));
                }
            } else {
                self.point(DecisionKind::Else, start(alternative));
                self.deeper(DecisionKind::If, at, |walker| {
                    walker.walk_children(alternative);
                });
            }
        }
    }

    /// `for` and `while`. Their `else` runs when the loop ends, and is no branch of its
    /// own.
    fn looping(&mut self, node: Node<'_>) {
        let at = start(node);
        self.point(DecisionKind::Loop, at);
        for field in ["left", "right", "condition"] {
            self.walk_field(node, field);
        }
        if let Some(body) = node.child_by_field_name("body") {
            self.deeper(DecisionKind::Loop, at, |walker| walker.walk(body));
        }
        if let Some(alternative) = node.child_by_field_name("alternative") {
            self.deeper(DecisionKind::Loop, at, |walker| {
                walker.walk_children(alternative);
            });
        }
    }

    /// `match` chooses one `case`: each after the first is one more way out, and so is
    /// each guard.
    fn matching(&mut self, node: Node<'_>) {
        let at = start(node);
        self.point(DecisionKind::Match, at);
        let mut cursor = node.walk();
        let subjects: Vec<Node<'_>> = node
            .children_by_field_name("subject", &mut cursor)
            .collect();
        for subject in subjects {
            self.walk(subject);
        }
        let cases: Vec<Node<'_>> = node
            .child_by_field_name("body")
            .map(named_children)
            .unwrap_or_default()
            .into_iter()
            .filter(|case| case.kind() == "case_clause")
            .collect();
        for (number, case) in cases.into_iter().enumerate() {
            if number > 0 {
                self.point(DecisionKind::MatchArm, start(case));
            }
            if let Some(guard) = case.child_by_field_name("guard") {
                self.point(DecisionKind::Guard, start(guard));
                self.walk(guard);
            }
            if let Some(consequence) = case.child_by_field_name("consequence") {
                self.deeper(DecisionKind::Match, at, |walker| walker.walk(consequence));
            }
        }
    }

    /// `try`: each `except` is a way out. `else` and `finally` are not.
    fn attempt(&mut self, node: Node<'_>) {
        for child in named_children(node) {
            if !matches!(child.kind(), "except_clause" | "except_group_clause") {
                self.walk(child);
                continue;
            }
            let at = start(child);
            self.point(DecisionKind::Catch, at);
            for part in named_children(child) {
                if part.kind() == "block" {
                    self.deeper(DecisionKind::Catch, at, |walker| walker.walk(part));
                } else {
                    self.walk(part);
                }
            }
        }
    }

    /// `a if test else b`.
    fn ternary(&mut self, node: Node<'_>) {
        let at = start(node);
        self.point(DecisionKind::Ternary, at);
        let parts = named_children(node);
        if let [chosen, test, otherwise] = parts.as_slice() {
            self.walk(*test);
            let (chosen, otherwise) = (*chosen, *otherwise);
            self.deeper(DecisionKind::Ternary, at, |walker| {
                walker.walk(chosen);
                walker.walk(otherwise);
            });
        } else {
            self.walk_children(node);
        }
    }

    /// Each `for` and `if` of a comprehension adds 1, with no nesting, since a
    /// comprehension reads on one line.
    fn comprehension(&mut self, node: Node<'_>) {
        for child in named_children(node) {
            match child.kind() {
                "for_in_clause" => self.flat_point(DecisionKind::Loop, start(child)),
                "if_clause" => self.flat_point(DecisionKind::If, start(child)),
                _ => {}
            }
            self.walk(child);
        }
    }

    fn logical_operator(&self, node: Node<'_>) -> Option<(u32, &'t str)> {
        if node.kind() != "boolean_operator" {
            return None;
        }
        let operator = node.child_by_field_name("operator")?;
        Some((start(operator), self.text_of(operator)))
    }

    /// Every logical operator is a branch, and each run of the same one adds a point.
    fn logical(&mut self, top: Node<'_>) {
        let mut operators = Vec::new();
        let mut operands = Vec::new();
        let mut pending = vec![top];
        while let Some(node) = pending.pop() {
            let inner = if node.kind() == "parenthesized_expression" {
                named_children(node)
                    .first()
                    .copied()
                    .filter(|inner| inner.kind() == "boolean_operator")
            } else {
                None
            };
            if let Some(inner) = inner {
                pending.push(inner);
                continue;
            }
            match self.logical_operator(node) {
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

    fn point(&mut self, kind: DecisionKind, offset: u32) {
        let nesting = self.frame.as_ref().map_or(0, |frame| frame.nesting);
        self.point_at(kind, offset, nesting);
    }

    /// A point that nesting does not make harder to read.
    fn flat_point(&mut self, kind: DecisionKind, offset: u32) {
        self.point_at(kind, offset, 0);
    }

    fn point_at(&mut self, kind: DecisionKind, offset: u32, nesting: u32) {
        let line = self.lines.line_of(offset);
        if let Some(frame) = &mut self.frame {
            frame.decisions.push(DecisionPoint {
                kind,
                line,
                nesting,
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

    /// A lambda is one level deeper to read, and adds no control structure.
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
        extract(&RelativePath::new("src/a.py"), text)
    }

    fn symbol<'f>(facts: &'f FileFacts, name: &str) -> &'f SymbolFact {
        facts
            .symbols
            .iter()
            .find(|symbol| symbol.name == name)
            .expect("the symbol")
    }

    fn named(facts: &FileFacts) -> Vec<(&str, SymbolKind, Option<&str>)> {
        facts
            .symbols
            .iter()
            .map(|symbol| {
                let parent = symbol
                    .parent
                    .and_then(|index| facts.symbols.get(index))
                    .map(|parent| parent.name.as_str());
                (symbol.name.as_str(), symbol.kind, parent)
            })
            .collect()
    }

    fn body(text: &str, name: &str) -> BodyFacts {
        symbol(&facts(text), name).body.clone().expect("a body")
    }

    fn references(text: &str, name: &str) -> Vec<String> {
        symbol(&facts(text), name)
            .references
            .iter()
            .map(|reference| format!("{:?} {}", reference.kind, reference.path.join(".")))
            .collect()
    }

    #[test]
    fn a_class_holds_its_methods_and_every_field_it_sets() {
        let found = facts(
            "class C(Base):\n    a: int = 1\n    def __init__(self):\n        self.b = 2\n        self.a = 3\n    def x(self):\n        return self.b\n",
        );
        assert_eq!(
            named(&found),
            [
                ("C", SymbolKind::Class, None),
                ("a", SymbolKind::Field, Some("C")),
                ("__init__", SymbolKind::Method, Some("C")),
                ("x", SymbolKind::Method, Some("C")),
                ("b", SymbolKind::Field, Some("C")),
            ]
        );
        assert_eq!(
            symbol(&found, "x")
                .body
                .as_ref()
                .map(|b| b.fields_used.clone()),
            Some(vec!["b".to_owned()])
        );
        assert!(!symbol(&found, "__init__").member.instance);
        assert!(symbol(&found, "x").member.instance);
    }

    #[test]
    fn protocols_abstract_bases_and_enums_are_kinds_of_their_own() {
        let found = facts(
            "class P(Protocol): pass\nclass A(abc.ABC): pass\nclass M(metaclass=ABCMeta): pass\nclass E(enum.Enum):\n    X = 1\n",
        );
        let kinds: Vec<SymbolKind> = ["P", "A", "M", "E"]
            .iter()
            .map(|name| symbol(&found, name).kind)
            .collect();
        assert_eq!(
            kinds,
            [
                SymbolKind::Interface,
                SymbolKind::Interface,
                SymbolKind::Interface,
                SymbolKind::Enum
            ]
        );
    }

    #[test]
    fn module_names_are_constants_aliases_or_functions() {
        let found = facts(
            "LIMIT = 1\ncount = 0\ncount = 1\nA: TypeAlias = int\ntype B = str\nf = lambda: 1\n",
        );
        let kinds: Vec<(&str, SymbolKind)> = found
            .symbols
            .iter()
            .map(|s| (s.name.as_str(), s.kind))
            .collect();
        assert_eq!(
            kinds,
            [
                ("LIMIT", SymbolKind::Constant),
                ("count", SymbolKind::Constant),
                ("A", SymbolKind::Alias),
                ("B", SymbolKind::Alias),
                ("f", SymbolKind::Function),
            ]
        );
    }

    #[test]
    fn parameters_leave_out_self_cls_and_markers() {
        let text = "class C:\n    def x(self, a, /, b, *args, c, **kw): pass\n    @classmethod\n    def y(cls, a): pass\n    @staticmethod\n    def z(a): pass\n";
        assert_eq!(body(text, "x").parameters, 5);
        assert_eq!(body(text, "y").parameters, 1);
        assert_eq!(body(text, "z").parameters, 1);
    }

    #[test]
    fn calls_are_told_apart_by_what_they_call_on() {
        let text = "import shop.billing as b\nfrom .catalog import Item\nclass C:\n    def x(self, i: Item):\n        self.y()\n        b.total(i)\n        Item(1)\n        i.price()\n";
        assert_eq!(
            references(text, "x"),
            [
                "Type Item",
                "SelfCall y",
                "Call b.total",
                "Call Item",
                "MethodCall price",
            ]
        );
    }

    #[test]
    fn complexity_reads_branches_loops_and_comprehensions() {
        let kinds = |text: &str| -> Vec<(DecisionKind, u32)> {
            body(text, "f")
                .decisions
                .iter()
                .map(|point| (point.kind, point.nesting))
                .collect()
        };
        assert_eq!(
            kinds(
                "def f(a):\n    if a:\n        pass\n    elif b:\n        pass\n    else:\n        pass\n"
            ),
            [
                (DecisionKind::If, 0),
                (DecisionKind::ElseIf, 0),
                (DecisionKind::Else, 0)
            ]
        );
        assert_eq!(
            kinds("def f(a):\n    if a:\n        return [x for x in a if x]\n"),
            [
                (DecisionKind::If, 0),
                (DecisionKind::Loop, 0),
                (DecisionKind::If, 0)
            ]
        );
        assert_eq!(
            kinds(
                "def f(a):\n    try:\n        pass\n    except A:\n        pass\n    except B:\n        pass\n"
            ),
            [(DecisionKind::Catch, 0), (DecisionKind::Catch, 0)]
        );
    }

    #[test]
    fn imports_say_what_they_bring_in_and_from_where() {
        let found = facts(
            "import shop.billing as b\nimport os\nfrom . import billing\nfrom .catalog import Item as I, Priced\nfrom x import *\n",
        );
        let imports: Vec<String> = found
            .imports
            .iter()
            .map(|import| {
                format!(
                    "{} {} {}",
                    import.alias.as_deref().unwrap_or("*"),
                    import.path.join("."),
                    import.from.as_deref().unwrap_or_default()
                )
            })
            .collect();
        assert_eq!(
            imports,
            [
                "b  shop.billing",
                "os  os",
                "billing billing .",
                "I Item .catalog",
                "Priced Priced .catalog",
                "*  x",
            ]
        );
    }
}

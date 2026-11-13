//! Walks a parsed Elixir file: finds its modules and what they hold, and describes what
//! makes each function complex.
//!
//! Elixir is all calls: `defmodule`, `def` and `if` are calls the grammar does not tell
//! apart, so this reads them by name. A module is named after itself. Every clause of one
//! name and arity is one function, named as Elixir writes it, `total/1`. A module with
//! `defstruct` also holds a struct named after the last part of its name, and a protocol
//! holds an interface the same way. A protocol's implementation adds its functions to
//! the struct of the type it is for, wherever that is written.
//!
//! Aliases are worked out here, so every module a fact names is its full name.

use crate::tree::{child_of_kind, end, named_children, start, text};
use cyclops_domain::facts::{
    BodyFacts, DecisionKind, DecisionPoint, Deepest, Extension, Import, MemberFacts, Reference,
    ReferenceKind, SymbolFact,
};
use cyclops_domain::lines::LineMap;
use cyclops_domain::symbol::{CodeRole, Span, SymbolKind, Visibility};
use std::collections::HashMap;
use tree_sitter::Node;

/// The calls that define a function, a macro or a guard.
const DEFINITIONS: [&str; 7] = [
    "def",
    "defp",
    "defmacro",
    "defmacrop",
    "defguard",
    "defguardp",
    "defdelegate",
];

/// The attributes Elixir itself reads. Any other attribute that holds a value is a
/// constant of its module.
const RESERVED: [&str; 26] = [
    "doc",
    "moduledoc",
    "typedoc",
    "spec",
    "type",
    "typep",
    "opaque",
    "callback",
    "macrocallback",
    "optional_callbacks",
    "behaviour",
    "impl",
    "derive",
    "enforce_keys",
    "compile",
    "deprecated",
    "dialyzer",
    "external_resource",
    "on_load",
    "on_definition",
    "before_compile",
    "after_compile",
    "after_verify",
    "vsn",
    "file",
    "since",
];

/// The import a file makes with `import`, whose functions it may call by name alone.
pub(crate) const IMPORTED_NAMES: &str = "*";

/// What the code being read is inside.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Context {
    /// Outside any module.
    File,
    Module,
    /// A protocol, whose functions belong to its interface.
    Protocol {
        interface: usize,
    },
    /// An implementation of a protocol, whose functions belong to the type it is for.
    Implementation {
        target: String,
    },
}

struct Scope {
    /// The module, if the scope is inside one.
    module: Option<usize>,
    /// The full name of the module, or of the implementation.
    name: String,
    context: Context,
    /// What each alias stands for.
    aliases: HashMap<String, String>,
}

/// The function being walked.
#[derive(Default)]
struct Frame {
    /// Its name and arity, as `total/1`.
    key: String,
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

/// One clause of a function.
struct Clause<'n> {
    node: Node<'n>,
    parameters: Vec<Node<'n>>,
    guard: Option<Node<'n>>,
    body: Option<Node<'n>>,
}

/// Every clause of one name and arity.
struct Function<'n> {
    name: String,
    arity: usize,
    visibility: Visibility,
    clauses: Vec<Clause<'n>>,
}

impl Function<'_> {
    fn key(&self) -> String {
        format!("{}/{}", self.name, self.arity)
    }
}

/// The operator of an operator node: the one child the grammar does not name.
fn operator(node: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = node.walk();
    node.children(&mut cursor).find(|child| !child.is_named())
}

fn operator_text<'t>(node: Node<'_>, source: &'t str) -> Option<&'t str> {
    operator(node).map(|operator| text(operator, source))
}

/// The name of an attribute such as `@doc "..."` or `@limit 10`.
pub(crate) fn attribute_name<'t>(node: Node<'_>, source: &'t str) -> Option<&'t str> {
    if node.kind() != "unary_operator" || operator_text(node, source)? != "@" {
        return None;
    }
    let operand = node.child_by_field_name("operand")?;
    match operand.kind() {
        "call" => Some(text(operand.child_by_field_name("target")?, source)),
        "identifier" => Some(text(operand, source)),
        _ => None,
    }
}

/// The arguments of a call.
fn arguments(node: Node<'_>) -> Vec<Node<'_>> {
    child_of_kind(node, "arguments")
        .map(named_children)
        .unwrap_or_default()
}

fn do_block(node: Node<'_>) -> Option<Node<'_>> {
    child_of_kind(node, "do_block")
}

/// The clauses of a block such as a `case`, an `fn` or a `rescue`.
fn stab_clauses(node: Node<'_>) -> Vec<Node<'_>> {
    named_children(node)
        .into_iter()
        .filter(|child| child.kind() == "stab_clause")
        .collect()
}

fn last_part(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
}

impl<'l, 't> Extractor<'l, 't> {
    pub(crate) fn new(lines: &'l LineMap, text: &'t str, role: CodeRole) -> Self {
        Self {
            lines,
            text,
            role,
            symbols: Vec::new(),
            imports: Vec::new(),
            extensions: Vec::new(),
            scopes: vec![Scope {
                module: None,
                name: String::new(),
                context: Context::File,
                aliases: HashMap::new(),
            }],
            parent: None,
            frame: None,
        }
    }

    pub(crate) fn into_parts(self) -> (Vec<SymbolFact>, Vec<Import>, Vec<Extension>) {
        (self.symbols, self.imports, self.extensions)
    }

    fn text_of(&self, node: Node<'_>) -> &'t str {
        text(node, self.text)
    }

    fn call_name(&self, node: Node<'_>) -> Option<&'t str> {
        if node.kind() != "call" {
            return None;
        }
        let target = node.child_by_field_name("target")?;
        (target.kind() == "identifier").then(|| self.text_of(target))
    }

    /// The value of a keyword argument, such as `for:` in `defimpl P, for: T`.
    fn keyword_value<'n>(&self, node: Node<'n>, key: &str) -> Option<Node<'n>> {
        arguments(node)
            .into_iter()
            .filter(|argument| argument.kind() == "keywords")
            .flat_map(named_children)
            .find(|pair| {
                pair.child_by_field_name("key")
                    .is_some_and(|written| self.keyword(written) == key)
            })
            .and_then(|pair| pair.child_by_field_name("value"))
    }

    /// A keyword as a name: `for` for `for: `.
    fn keyword(&self, node: Node<'_>) -> &'t str {
        self.text_of(node).trim().trim_end_matches(':')
    }

    fn scope(&self) -> Option<&Scope> {
        self.scopes.last()
    }

    /// The module the code being read is in, if any.
    fn module(&self) -> Option<usize> {
        self.scope().and_then(|scope| scope.module)
    }

    /// The full name of a module as written here, with aliases and `__MODULE__` worked
    /// out.
    fn expand(&self, written: &str) -> String {
        if let Some(rest) = written.strip_prefix("__MODULE__") {
            let current = self
                .scope()
                .map(|scope| scope.name.as_str())
                .unwrap_or_default();
            return format!("{current}{rest}");
        }
        let (first, rest) = written
            .split_once('.')
            .map_or((written, ""), |(first, rest)| (first, rest));
        let aliased = self
            .scopes
            .iter()
            .rev()
            .find_map(|scope| scope.aliases.get(first));
        match aliased {
            Some(full) if rest.is_empty() => full.clone(),
            Some(full) => format!("{full}.{rest}"),
            None => written.to_owned(),
        }
    }

    /// The full name of a module a node names, if it names one.
    fn module_named(&self, node: Node<'_>) -> Option<String> {
        let names_module = match node.kind() {
            "alias" => true,
            "identifier" => self.text_of(node) == "__MODULE__",
            "dot" => node
                .child_by_field_name("left")
                .is_some_and(|left| self.module_named(left).is_some()),
            _ => false,
        };
        names_module.then(|| self.expand(self.text_of(node)))
    }

    fn alias(&mut self, short: &str, full: &str) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.aliases.insert(short.to_owned(), full.to_owned());
        }
    }

    /// Says that the module being read depends on another.
    fn depends_on(&mut self, module: String, imported: bool) {
        self.imports.push(Import {
            alias: imported.then(|| IMPORTED_NAMES.to_owned()),
            path: Vec::new(),
            from: Some(module),
            scope: self.module(),
        });
    }

    pub(crate) fn file(&mut self, root: Node<'_>) {
        for node in named_children(root) {
            self.statement(node);
        }
    }

    /// Something written directly in a module, or in the file.
    fn statement(&mut self, node: Node<'_>) {
        if node.kind() == "unary_operator" {
            self.attribute(node);
            return;
        }
        match self.call_name(node) {
            Some("defmodule") => self.define_module(node, false),
            Some("defprotocol") => self.define_module(node, true),
            Some("defimpl") => self.implementation(node),
            Some("defstruct" | "defexception") => self.structure(node),
            Some(which @ ("alias" | "import" | "require" | "use")) => self.dependency(node, which),
            _ => {}
        }
    }

    fn define_module(&mut self, node: Node<'_>, protocol: bool) {
        let Some(written) = arguments(node).first().map(|name| self.text_of(*name)) else {
            return;
        };
        let outer = self
            .scope()
            .filter(|scope| scope.module.is_some())
            .map(|scope| scope.name.clone());
        let full = match outer {
            Some(outer) if !written.starts_with("__MODULE__") => {
                // A module inside another is an alias there.
                let first = written.split('.').next().unwrap_or(written);
                self.alias(first, &format!("{outer}.{first}"));
                format!("{outer}.{written}")
            }
            _ => self.expand(written),
        };
        let index = self.push(&full, SymbolKind::Module, Visibility::Public, node, None);
        let context = if protocol {
            let interface = self.push(
                last_part(&full),
                SymbolKind::Interface,
                Visibility::Public,
                node,
                Some(index),
            );
            Context::Protocol { interface }
        } else {
            Context::Module
        };
        self.scopes.push(Scope {
            module: Some(index),
            name: full,
            context,
            aliases: HashMap::new(),
        });
        if let Some(block) = do_block(node) {
            self.body(block);
        }
        self.scopes.pop();
    }

    /// `defimpl Priced, for: Item`, or inside `Item` without `for:`.
    fn implementation(&mut self, node: Node<'_>) {
        let Some(protocol) = arguments(node)
            .first()
            .and_then(|written| self.module_named(*written))
        else {
            return;
        };
        let target = match self.keyword_value(node, "for") {
            Some(written) => self.module_named(written),
            None => self
                .scope()
                .filter(|scope| scope.context == Context::Module)
                .map(|scope| scope.name.clone()),
        };
        let Some(target) = target else {
            return;
        };
        let start_line = self.lines.line_of(start(node));
        let end_line = self
            .lines
            .line_of(end(node).saturating_sub(1).max(start(node)));
        self.extensions.push(Extension {
            owner: vec![target.clone()],
            parent: self.module(),
            implements: Some(vec![protocol.clone()]),
            code_lines: self.lines.counts_between(start_line, end_line).code,
        });
        let module = self.module();
        self.scopes.push(Scope {
            module,
            name: format!("{protocol}.{target}"),
            context: Context::Implementation { target },
            aliases: HashMap::new(),
        });
        if let Some(block) = do_block(node) {
            self.body(block);
        }
        self.scopes.pop();
    }

    /// `defstruct [:a, b: 1]`: a struct named after the module, with its fields.
    fn structure(&mut self, node: Node<'_>) {
        let Some(scope) = self.scope() else {
            return;
        };
        let (Some(module), Context::Module) = (scope.module, &scope.context) else {
            return;
        };
        let name = last_part(&scope.name).to_owned();
        let index = self.push(
            &name,
            SymbolKind::Struct,
            Visibility::Public,
            node,
            Some(module),
        );
        for argument in arguments(node) {
            let items = match argument.kind() {
                "list" => named_children(argument),
                _ => vec![argument],
            };
            for item in items {
                match item.kind() {
                    "atom" => {
                        let field = self.text_of(item).trim_start_matches(':');
                        self.push(
                            field,
                            SymbolKind::Field,
                            Visibility::Public,
                            item,
                            Some(index),
                        );
                    }
                    "keywords" => {
                        for pair in named_children(item) {
                            if let Some(key) = pair.child_by_field_name("key") {
                                let field = self.keyword(key);
                                self.push(
                                    field,
                                    SymbolKind::Field,
                                    Visibility::Public,
                                    pair,
                                    Some(index),
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    /// `alias`, `import`, `require` and `use`. An alias also gives a short name.
    fn dependency(&mut self, node: Node<'_>, which: &str) {
        let Some(first) = arguments(node).first().copied() else {
            return;
        };
        let several = first.kind() == "dot"
            && first
                .child_by_field_name("right")
                .is_some_and(|right| right.kind() == "tuple");
        let named: Vec<(String, String)> = if several {
            // `alias Shop.{Invoice, Item}`
            let base = first
                .child_by_field_name("left")
                .and_then(|left| self.module_named(left));
            let parts = first
                .child_by_field_name("right")
                .map(named_children)
                .unwrap_or_default();
            base.map(|base| {
                parts
                    .into_iter()
                    .filter(|part| part.kind() == "alias")
                    .map(|part| {
                        let written = self.text_of(part);
                        (last_part(written).to_owned(), format!("{base}.{written}"))
                    })
                    .collect()
            })
            .unwrap_or_default()
        } else {
            let Some(full) = self.module_named(first) else {
                return;
            };
            let short = self.keyword_value(node, "as").map_or_else(
                || last_part(&full).to_owned(),
                |name| self.text_of(name).to_owned(),
            );
            vec![(short, full)]
        };
        for (short, full) in named {
            if which == "alias" {
                self.alias(&short, &full);
            }
            self.depends_on(full, which == "import");
        }
    }

    /// `@limit 10` is a constant of its module, and `@behaviour B` a dependency.
    fn attribute(&mut self, node: Node<'_>) {
        let Some(name) = attribute_name(node, self.text) else {
            return;
        };
        let operand = node.child_by_field_name("operand");
        let value = operand.and_then(|operand| arguments(operand).first().copied());
        if name == "behaviour" {
            if let Some(module) = value.and_then(|value| self.module_named(value)) {
                self.depends_on(module, false);
            }
            return;
        }
        let Some(module) = self.module() else {
            return;
        };
        let constant = format!("@{name}");
        let known = self
            .symbols
            .iter()
            .any(|symbol| symbol.parent == Some(module) && symbol.name == constant);
        if value.is_some() && !RESERVED.contains(&name) && !known {
            self.push(
                &constant,
                SymbolKind::Constant,
                Visibility::Private,
                node,
                Some(module),
            );
        }
    }

    /// The statements of a module, a protocol or an implementation. The clauses of a
    /// function are gathered first, since they make one function.
    fn body(&mut self, block: Node<'_>) {
        let mut functions: Vec<Function<'_>> = Vec::new();
        for node in named_children(block) {
            match self.clause(node) {
                Some((name, arity, visibility, clause)) => {
                    let known = functions
                        .iter_mut()
                        .find(|function| function.name == name && function.arity == arity);
                    match known {
                        Some(function) => function.clauses.push(clause),
                        None => functions.push(Function {
                            name,
                            arity,
                            visibility,
                            clauses: vec![clause],
                        }),
                    }
                }
                None => self.statement(node),
            }
        }
        for function in functions {
            self.function(&function);
        }
    }

    /// A clause of a function: its name, arity and visibility, and what it is made of.
    fn clause<'n>(&self, node: Node<'n>) -> Option<(String, usize, Visibility, Clause<'n>)> {
        let definition = self
            .call_name(node)
            .filter(|name| DEFINITIONS.contains(name))?;
        let first = arguments(node).first().copied()?;
        let when = first.kind() == "binary_operator"
            && operator_text(first, self.text).is_some_and(|operator| operator == "when");
        let (head, guard) = if when {
            (
                first.child_by_field_name("left")?,
                first.child_by_field_name("right"),
            )
        } else {
            (first, None)
        };
        let (name, parameters) = match head.kind() {
            "identifier" => (self.text_of(head), Vec::new()),
            "call" => (self.call_name(head)?, arguments(head)),
            _ => return None,
        };
        let written_body = do_block(node).or_else(|| self.keyword_value(node, "do"));
        // A guard defined with `defguard` is the guard itself.
        let (guard, body) = if definition.starts_with("defguard") {
            (None, guard)
        } else {
            (guard, written_body)
        };
        let visibility = if definition.ends_with('p') {
            Visibility::Private
        } else {
            Visibility::Public
        };
        let arity = parameters.len();
        Some((
            name.to_owned(),
            arity,
            visibility,
            Clause {
                node,
                parameters,
                guard,
                body,
            },
        ))
    }

    fn function(&mut self, function: &Function<'_>) {
        let (Some(first), Some(last)) = (function.clauses.first(), function.clauses.last()) else {
            return;
        };
        let context = self
            .scope()
            .map_or(Context::File, |scope| scope.context.clone());
        let (kind, parent, owner, member) = match context {
            Context::File => return,
            Context::Module => (
                SymbolKind::Function,
                self.module(),
                None,
                MemberFacts::default(),
            ),
            Context::Protocol { interface } => (
                SymbolKind::Method,
                Some(interface),
                None,
                MemberFacts {
                    instance: true,
                    for_interface: false,
                },
            ),
            Context::Implementation { target } => (
                SymbolKind::Method,
                self.module(),
                Some(vec![target]),
                MemberFacts {
                    instance: false,
                    for_interface: true,
                },
            ),
        };
        let key = function.key();
        let index = self.push_span(
            &key,
            kind,
            function.visibility,
            (start(first.node), end(last.node)),
            parent,
        );
        if let Some(symbol) = self.symbols.get_mut(index) {
            symbol.owner = owner;
            symbol.member = member;
        }
        if function.clauses.iter().all(|clause| clause.body.is_none()) {
            return;
        }
        let outer_frame = self.frame.replace(Frame {
            key,
            ..Frame::default()
        });
        let outer_parent = self.parent.replace(index);
        for (number, clause) in function.clauses.iter().enumerate() {
            if number > 0 {
                self.point(DecisionKind::MatchArm, start(clause.node));
            }
            for parameter in &clause.parameters {
                self.walk(*parameter);
            }
            if let Some(guard) = clause.guard {
                self.point(DecisionKind::Guard, start(guard));
                self.walk(guard);
            }
            if let Some(body) = clause.body {
                self.walk(body);
            }
        }
        self.parent = outer_parent;
        let finished = std::mem::replace(&mut self.frame, outer_frame);
        if let (Some(mut frame), Some(symbol)) = (finished, self.symbols.get_mut(index)) {
            frame.decisions.sort_by_key(|point| point.line);
            symbol.body = Some(BodyFacts {
                decisions: frame.decisions,
                parameters: u32::try_from(function.arity).unwrap_or(u32::MAX),
                max_nesting: frame.max_nesting,
                fields_used: Vec::new(),
                methods_called: Vec::new(),
                deepest: frame.deepest.map(|(_, line, path)| Deepest { line, path }),
            });
        }
    }

    fn push(
        &mut self,
        name: &str,
        kind: SymbolKind,
        visibility: Visibility,
        node: Node<'_>,
        parent: Option<usize>,
    ) -> usize {
        self.push_span(name, kind, visibility, (start(node), end(node)), parent)
    }

    fn push_span(
        &mut self,
        name: &str,
        kind: SymbolKind,
        visibility: Visibility,
        (from, to): (u32, u32),
        parent: Option<usize>,
    ) -> usize {
        let start_line = self.lines.line_of(from);
        let end_line = self.lines.line_of(to.saturating_sub(1).max(from));
        self.symbols.push(SymbolFact {
            name: name.to_owned(),
            parent,
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
        });
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
            "call" => self.call(node, 0),
            "binary_operator" => self.binary(node),
            "anonymous_function" => self.inside_closure(|walker| walker.clauses(node, None)),
            "map" => {
                let named = child_of_kind(node, "struct")
                    .and_then(|structure| named_children(structure).first().copied())
                    .and_then(|name| self.module_named(name));
                if let Some(module) = named {
                    self.reference(ReferenceKind::Type, vec![module]);
                }
                self.walk_children(node);
            }
            _ => self.walk_children(node),
        }
    }

    /// A call, with `extra` arguments a pipe passes it.
    fn call(&mut self, node: Node<'_>, extra: usize) {
        let Some(target) = node.child_by_field_name("target") else {
            self.walk_children(node);
            return;
        };
        let arguments = arguments(node);
        match target.kind() {
            "identifier" => match self.text_of(target) {
                "if" | "unless" => self.conditional(node),
                "case" | "cond" | "receive" => self.matching(node),
                "with" => self.with(node),
                "try" => self.attempt(node),
                "for" => {
                    let at = start(node);
                    self.point(DecisionKind::Loop, at);
                    self.deeper(DecisionKind::Loop, at, |walker| walker.walk_children(node));
                }
                name => {
                    let key = format!("{name}/{}", arguments.len() + extra);
                    if self.frame.as_ref().is_some_and(|frame| frame.key == key) {
                        self.recursion(start(target));
                    }
                    self.reference(ReferenceKind::Call, vec![key]);
                    self.walk_all(&arguments);
                    if let Some(block) = do_block(node) {
                        self.walk(block);
                    }
                }
            },
            "dot" => {
                let left = target.child_by_field_name("left");
                let right = target.child_by_field_name("right");
                let module = left.and_then(|left| self.module_named(left));
                match (module, right) {
                    (Some(module), Some(right)) if right.kind() == "identifier" => {
                        let key = format!("{}/{}", self.text_of(right), arguments.len() + extra);
                        self.reference(ReferenceKind::Call, vec![module, key]);
                    }
                    _ => {
                        if let Some(left) = left {
                            self.walk(left);
                        }
                    }
                }
                self.walk_all(&arguments);
                if let Some(block) = do_block(node) {
                    self.walk(block);
                }
            }
            _ => self.walk_children(node),
        }
    }

    fn walk_all(&mut self, nodes: &[Node<'_>]) {
        for node in nodes {
            self.walk(*node);
        }
    }

    fn binary(&mut self, node: Node<'_>) {
        match operator_text(node, self.text) {
            Some("and" | "or" | "&&" | "||") => self.logical(node),
            Some("|>") => {
                if let Some(left) = node.child_by_field_name("left") {
                    self.walk(left);
                }
                match node.child_by_field_name("right") {
                    Some(right) if right.kind() == "call" => self.call(right, 1),
                    Some(right) => self.walk(right),
                    None => {}
                }
            }
            _ => self.walk_children(node),
        }
    }

    fn is_logical(&self, node: Node<'_>) -> Option<(u32, &'t str)> {
        if node.kind() != "binary_operator" {
            return None;
        }
        let operator = operator(node)?;
        let written = self.text_of(operator);
        matches!(written, "and" | "or" | "&&" | "||").then(|| (start(operator), written))
    }

    /// Every logical operator is a branch, and each run of the same one adds a point.
    fn logical(&mut self, top: Node<'_>) {
        let mut operators = Vec::new();
        let mut operands = Vec::new();
        let mut pending = vec![top];
        while let Some(node) = pending.pop() {
            match self.is_logical(node) {
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
        self.walk_all(&operands);
    }

    /// `if` and `unless`, in either form: with `do ... else ... end`, or with `do:` and
    /// `else:`.
    fn conditional(&mut self, node: Node<'_>) {
        let at = start(node);
        self.point(DecisionKind::If, at);
        for argument in arguments(node) {
            if argument.kind() != "keywords" {
                self.walk(argument);
                continue;
            }
            for pair in named_children(argument) {
                let key = pair.child_by_field_name("key").map(|key| self.keyword(key));
                let Some(value) = pair.child_by_field_name("value") else {
                    continue;
                };
                if key == Some("else") {
                    self.point(DecisionKind::Else, start(pair));
                }
                self.deeper(DecisionKind::If, at, |walker| walker.walk(value));
            }
        }
        let Some(block) = do_block(node) else {
            return;
        };
        for child in named_children(block) {
            if child.kind() == "else_block" {
                self.point(DecisionKind::Else, start(child));
            }
            self.deeper(DecisionKind::If, at, |walker| walker.walk(child));
        }
    }

    /// `case`, `cond` and `receive` choose one clause, as a `match` does.
    fn matching(&mut self, node: Node<'_>) {
        let at = start(node);
        self.point(DecisionKind::Match, at);
        self.walk_all(&arguments(node));
        let Some(block) = do_block(node) else {
            return;
        };
        self.clauses(block, Some(at));
        for child in named_children(block) {
            if child.kind() != "stab_clause" {
                self.walk(child);
            }
        }
    }

    /// The clauses of a choice: each after the first is one more way out, and so is each
    /// guard. Inside a structure, what each clause does is one level deeper.
    fn clauses(&mut self, node: Node<'_>, structure: Option<u32>) {
        for (number, clause) in stab_clauses(node).into_iter().enumerate() {
            if number > 0 {
                self.point(DecisionKind::MatchArm, start(clause));
            }
            if let Some(left) = clause.child_by_field_name("left") {
                self.pattern(left);
            }
            if let Some(right) = clause.child_by_field_name("right") {
                match structure {
                    Some(at) => self.deeper(DecisionKind::Match, at, |walker| walker.walk(right)),
                    None => self.walk(right),
                }
            }
        }
    }

    /// The left of a clause: its patterns, and its guard if it has one.
    fn pattern(&mut self, left: Node<'_>) {
        let when = left.kind() == "binary_operator"
            && operator_text(left, self.text).is_some_and(|operator| operator == "when");
        if !when {
            self.walk(left);
            return;
        }
        if let Some(patterns) = left.child_by_field_name("left") {
            self.walk(patterns);
        }
        if let Some(guard) = left.child_by_field_name("right") {
            self.point(DecisionKind::Guard, start(guard));
            self.walk(guard);
        }
    }

    /// `with` reads as one structure, and each step can fail.
    fn with(&mut self, node: Node<'_>) {
        let at = start(node);
        self.point(DecisionKind::Match, at);
        for argument in arguments(node) {
            let step = argument.kind() == "binary_operator"
                && operator_text(argument, self.text).is_some_and(|operator| operator == "<-");
            if step {
                self.point(DecisionKind::Shortcut, start(argument));
            }
            if argument.kind() == "keywords" {
                self.deeper(DecisionKind::Match, at, |walker| walker.walk(argument));
            } else {
                self.walk(argument);
            }
        }
        let Some(block) = do_block(node) else {
            return;
        };
        for child in named_children(block) {
            if child.kind() == "else_block" {
                self.clauses(child, Some(at));
            } else {
                self.deeper(DecisionKind::Match, at, |walker| walker.walk(child));
            }
        }
    }

    /// `try`: each `rescue` or `catch` clause is a way out.
    fn attempt(&mut self, node: Node<'_>) {
        let Some(block) = do_block(node) else {
            return;
        };
        for child in named_children(block) {
            if !matches!(child.kind(), "rescue_block" | "catch_block") {
                self.walk(child);
                continue;
            }
            for clause in stab_clauses(child) {
                let at = start(clause);
                self.point(DecisionKind::Catch, at);
                if let Some(left) = clause.child_by_field_name("left") {
                    self.walk(left);
                }
                if let Some(right) = clause.child_by_field_name("right") {
                    self.deeper(DecisionKind::Catch, at, |walker| walker.walk(right));
                }
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

    /// An `fn` is one level deeper to read, and adds no control structure.
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

pub(crate) struct Extractor<'l, 't> {
    lines: &'l LineMap,
    text: &'t str,
    role: CodeRole,
    symbols: Vec<SymbolFact>,
    imports: Vec<Import>,
    extensions: Vec<Extension>,
    /// Where the code being read is, from the outside in.
    scopes: Vec<Scope>,
    /// The symbol being filled.
    parent: Option<usize>,
    frame: Option<Frame>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::extract;
    use cyclops_domain::facts::FileFacts;
    use cyclops_domain::path::RelativePath;

    fn facts(text: &str) -> FileFacts {
        extract(&RelativePath::new("lib/a.ex"), text)
    }

    fn symbol<'f>(facts: &'f FileFacts, name: &str) -> &'f SymbolFact {
        facts
            .symbols
            .iter()
            .find(|symbol| symbol.name == name)
            .expect("the symbol")
    }

    fn named(facts: &FileFacts) -> Vec<(&str, Option<&str>)> {
        facts
            .symbols
            .iter()
            .map(|symbol| {
                let parent = symbol
                    .parent
                    .and_then(|index| facts.symbols.get(index))
                    .map(|parent| parent.name.as_str());
                (symbol.name.as_str(), parent)
            })
            .collect()
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

    fn calls(text: &str, name: &str) -> Vec<String> {
        let facts = facts(text);
        symbol(&facts, name)
            .references
            .iter()
            .map(|reference| reference.path.join(" "))
            .collect()
    }

    #[test]
    fn modules_hold_their_functions_structs_and_constants() {
        let facts = facts(
            "defmodule Shop.Order do\n  @limit 10\n  @doc \"d\"\n  defstruct [:a, b: 1]\n  def new, do: 1\n  defp add(a, b), do: a\nend\n",
        );
        assert_eq!(
            named(&facts),
            [
                ("Shop.Order", None),
                ("@limit", Some("Shop.Order")),
                ("Order", Some("Shop.Order")),
                ("a", Some("Order")),
                ("b", Some("Order")),
                ("new/0", Some("Shop.Order")),
                ("add/2", Some("Shop.Order")),
            ]
        );
        assert_eq!(symbol(&facts, "add/2").visibility, Visibility::Private);
    }

    #[test]
    fn clauses_of_one_name_and_arity_are_one_function() {
        let facts = facts(
            "defmodule M do\n  def f(0), do: 0\n  def f(n) when n > 1, do: n\n  def f(a, b), do: b\nend\n",
        );
        let names: Vec<&str> = facts.symbols.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["M", "f/1", "f/2"]);
        assert_eq!(
            kinds(
                "defmodule M do\n  def f(0), do: 0\n  def f(n) when n > 1, do: n\nend\n",
                "f/1"
            ),
            [DecisionKind::MatchArm, DecisionKind::Guard]
        );
    }

    #[test]
    fn a_nested_module_is_named_inside_its_module_and_aliased_there() {
        let facts = facts(
            "defmodule A do\n  defmodule B do\n    def f, do: 1\n  end\n  def g, do: B.f()\nend\n",
        );
        let names: Vec<&str> = facts.symbols.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["A", "A.B", "f/0", "g/0"]);
        let facts_text =
            "defmodule A do\n  defmodule B do\n    def f, do: 1\n  end\n  def g, do: B.f()\nend\n";
        assert_eq!(calls(facts_text, "g/0"), ["A.B f/0"]);
    }

    #[test]
    fn aliases_give_full_names_to_calls_and_structs() {
        let text = "defmodule Shop.Order do\n  alias Shop.{Invoice, Item}\n  alias Shop.Billing, as: B\n  def add(%Item{} = item), do: item |> Invoice.new(0) |> B.send()\n  def me, do: %__MODULE__{}\nend\n";
        assert_eq!(
            calls(text, "add/1"),
            ["Shop.Item", "Shop.Invoice new/2", "Shop.Billing send/1"]
        );
        assert_eq!(calls(text, "me/0"), ["Shop.Order"]);
        let facts = facts(text);
        let imported: Vec<&str> = facts
            .imports
            .iter()
            .filter_map(|import| import.from.as_deref())
            .collect();
        assert_eq!(imported, ["Shop.Invoice", "Shop.Item", "Shop.Billing"]);
    }

    #[test]
    fn a_protocol_holds_an_interface_and_an_implementation_adds_to_its_type() {
        let facts = facts(
            "defprotocol P do\n  def x(a)\nend\ndefmodule M do\n  defstruct [:a]\n  defimpl P do\n    def x(m), do: m.a\n  end\nend\n",
        );
        assert_eq!(symbol(&facts, "P").kind, SymbolKind::Module);
        let interface = facts
            .symbols
            .iter()
            .find(|symbol| symbol.kind == SymbolKind::Interface)
            .expect("an interface");
        assert_eq!(interface.name, "P");
        let methods: Vec<(&str, Option<&Vec<String>>)> = facts
            .symbols
            .iter()
            .filter(|symbol| symbol.kind == SymbolKind::Method)
            .map(|symbol| (symbol.name.as_str(), symbol.owner.as_ref()))
            .collect();
        assert_eq!(methods.len(), 2);
        assert_eq!(
            methods.get(1).and_then(|(_, owner)| owner.cloned()),
            Some(vec!["M".to_owned()])
        );
        let extension = facts.extensions.first().expect("an implementation");
        assert_eq!(extension.owner, ["M"]);
        assert_eq!(extension.implements, Some(vec!["P".to_owned()]));
    }

    #[test]
    fn complexity_reads_if_case_with_and_rescue() {
        assert_eq!(
            kinds(
                "defmodule M do\n  def f(a) do\n    if a do 1 else 2 end\n  end\nend\n",
                "f/1"
            ),
            [DecisionKind::If, DecisionKind::Else]
        );
        assert_eq!(
            kinds(
                "defmodule M do def f(a) do case a do x when x > 1 -> 1; _ -> 0 end end end",
                "f/1"
            ),
            [
                DecisionKind::Match,
                DecisionKind::Guard,
                DecisionKind::MatchArm
            ]
        );
        assert_eq!(
            kinds(
                "defmodule M do def f(a) do with {:ok, x} <- a do x end end end",
                "f/1"
            ),
            [DecisionKind::Match, DecisionKind::Shortcut]
        );
        assert_eq!(
            kinds(
                "defmodule M do def f(a) do try do a.() rescue _ -> 1 end end end",
                "f/1"
            ),
            [DecisionKind::Catch]
        );
    }

    #[test]
    fn calling_itself_with_its_arity_is_recursion() {
        assert_eq!(
            kinds("defmodule M do def f(n), do: f(n) + f(n, 1) end", "f/1"),
            [DecisionKind::Recursion]
        );
    }
}

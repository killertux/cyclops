//! Walks a parsed file: finds its symbols, and describes what makes each function
//! complex. Everything that is not a symbol counts towards the function around it.

use cyclops_domain::facts::{BodyFacts, DecisionKind, DecisionPoint, SymbolFact};
use cyclops_domain::lines::LineMap;
use cyclops_domain::symbol::{CodeRole, Span, SymbolKind, Visibility};
use oxc_ast::ast::{
    ArrowFunctionExpression, AssignmentExpression, BindingPattern, BreakStatement, CallExpression,
    CatchClause, Class, ClassElement, ComputedMemberExpression, ConditionalExpression,
    ContinueStatement, Declaration, DoWhileStatement, ExportDefaultDeclarationKind, Expression,
    ForInStatement, ForOfStatement, ForStatement, FormalParameters, Function, FunctionBody,
    FunctionType, IfStatement, LogicalExpression, MethodDefinition, PrivateFieldExpression,
    Program, PropertyDefinition, PropertyKey, Statement, StaticMemberExpression, SwitchStatement,
    TSAccessibility, TSInterfaceDeclaration, TSNamespaceDeclaration, TSNamespaceDeclarationBody,
    TSSignature, TSType, TSTypeAnnotation, TryStatement, VariableDeclaration, VariableDeclarator,
    WhileStatement,
};
use oxc_ast_visit::{Visit, walk};
use oxc_span::GetSpan;
use oxc_syntax::operator::LogicalOperator;
use oxc_syntax::scope::ScopeFlags;
use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;

/// The function being walked.
struct Frame {
    name: String,
    is_method: bool,
    decisions: Vec<DecisionPoint>,
    max_nesting: u32,
    /// How deep the walk is for cognitive complexity, which also counts closures.
    nesting: u32,
    /// How many control structures are around the walk.
    depth: u32,
    recursive: bool,
}

/// What every function needs, however it is written.
struct Callable<'n, 'a> {
    name: String,
    kind: SymbolKind,
    visibility: Visibility,
    start: u32,
    end: u32,
    params: &'n FormalParameters<'a>,
    body: Option<CallableBody<'n, 'a>>,
}

enum CallableBody<'n, 'a> {
    Function(&'n FunctionBody<'a>),
    Arrow(&'n ArrowFunctionExpression<'a>),
}

pub(crate) struct Extractor<'l, 'a> {
    lines: &'l LineMap,
    text: &'a str,
    role: CodeRole,
    symbols: Vec<SymbolFact>,
    /// The symbol being filled.
    parent: Option<usize>,
    frame: Option<Frame>,
    /// Functions declared but not written yet: the overloads of what comes next.
    declared: HashMap<(Option<usize>, String), usize>,
    /// What `export { name }` and `export default name` make public.
    exported: HashSet<String>,
    marker: PhantomData<&'a ()>,
}

fn visibility_of(accessibility: Option<TSAccessibility>) -> Visibility {
    match accessibility {
        Some(TSAccessibility::Private) => Visibility::Private,
        Some(TSAccessibility::Protected) => Visibility::Restricted,
        Some(TSAccessibility::Public) | None => Visibility::Public,
    }
}

/// A `#name` is private whatever else it says.
fn member_visibility(accessibility: Option<TSAccessibility>, key: &PropertyKey<'_>) -> Visibility {
    if matches!(key, PropertyKey::PrivateIdentifier(_)) {
        Visibility::Private
    } else {
        visibility_of(accessibility)
    }
}

fn key_name(key: &PropertyKey<'_>, text: &str) -> String {
    if let PropertyKey::PrivateIdentifier(identifier) = key {
        return format!("#{}", identifier.name);
    }
    let name = key.static_name();
    if let Some(name) = name.as_ref().and_then(|name| name.as_str()) {
        return name.to_owned();
    }
    let span = key.span();
    let (start, end) = (usize::try_from(span.start), usize::try_from(span.end));
    let written = start
        .ok()
        .zip(end.ok())
        .and_then(|(start, end)| text.get(start..end));
    written.unwrap_or("computed").to_owned()
}

fn binding_name(pattern: &BindingPattern<'_>) -> Option<String> {
    pattern
        .get_binding_identifier()
        .map(|identifier| identifier.name.to_string())
}

fn parameter_count(params: &FormalParameters<'_>) -> u32 {
    let rest = u32::from(params.rest.is_some());
    params.items.iter().fold(rest, |count, _| count + 1)
}

impl<'l, 'a> Extractor<'l, 'a> {
    pub(crate) fn new(lines: &'l LineMap, text: &'a str, role: CodeRole) -> Self {
        Self {
            lines,
            text,
            role,
            symbols: Vec::new(),
            parent: None,
            frame: None,
            declared: HashMap::new(),
            exported: HashSet::new(),
            marker: PhantomData,
        }
    }

    pub(crate) fn into_symbols(self) -> Vec<SymbolFact> {
        self.symbols
    }

    pub(crate) fn program(&mut self, program: &Program<'a>) {
        self.statements(&program.body);
        for symbol in &mut self.symbols {
            if symbol.parent.is_none() && self.exported.contains(&symbol.name) {
                symbol.visibility = Visibility::Public;
            }
        }
    }

    fn statements(&mut self, statements: &[Statement<'a>]) {
        for statement in statements {
            self.statement(statement);
        }
    }

    fn statement(&mut self, statement: &Statement<'a>) {
        match statement {
            Statement::ExportDeclaration(export) => {
                self.declaration(&export.declaration, Visibility::Public);
            }
            Statement::ExportNamedDeclaration(export) => {
                let names = export.specifiers.iter();
                self.exported
                    .extend(names.map(|specifier| specifier.local.name().to_string()));
            }
            Statement::ExportDefaultDeclaration(export) => {
                self.default_export(&export.declaration);
            }
            other => {
                if let Some(declaration) = other.as_declaration() {
                    self.declaration(declaration, Visibility::Private);
                }
            }
        }
    }

    fn default_export(&mut self, export: &ExportDefaultDeclarationKind<'a>) {
        const DEFAULT: &str = "default";
        match export {
            ExportDefaultDeclarationKind::FunctionDeclaration(function) => {
                let name = function.id.as_ref().map_or(DEFAULT, |id| id.name.as_str());
                self.function(name, function, Visibility::Public);
            }
            ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                let name = class.id.as_ref().map_or(DEFAULT, |id| id.name.as_str());
                self.class(name, class, Visibility::Public);
            }
            ExportDefaultDeclarationKind::TSInterfaceDeclaration(interface) => {
                self.interface(interface, Visibility::Public);
            }
            ExportDefaultDeclarationKind::Identifier(identifier) => {
                self.exported.insert(identifier.name.to_string());
            }
            ExportDefaultDeclarationKind::ArrowFunctionExpression(arrow) => {
                self.callable(Callable {
                    name: DEFAULT.to_owned(),
                    kind: SymbolKind::Function,
                    visibility: Visibility::Public,
                    start: arrow.span.start,
                    end: arrow.span.end,
                    params: &arrow.params,
                    body: Some(CallableBody::Arrow(arrow)),
                });
            }
            _ => {}
        }
    }

    fn declaration(&mut self, declaration: &Declaration<'a>, visibility: Visibility) {
        match declaration {
            Declaration::VariableDeclaration(variables) => {
                self.variables(variables, visibility);
            }
            Declaration::FunctionDeclaration(function) => {
                if let Some(id) = &function.id {
                    self.function(id.name.as_str(), function, visibility);
                }
            }
            Declaration::ClassDeclaration(class) => {
                if let Some(id) = &class.id {
                    self.class(id.name.as_str(), class, visibility);
                }
            }
            Declaration::TSTypeAliasDeclaration(alias) => {
                self.plain(
                    alias.id.name.as_str(),
                    SymbolKind::Alias,
                    visibility,
                    alias.span,
                );
            }
            Declaration::TSInterfaceDeclaration(interface) => {
                self.interface(interface, visibility);
            }
            Declaration::TSEnumDeclaration(item) => {
                self.plain(
                    item.id.name.as_str(),
                    SymbolKind::Enum,
                    visibility,
                    item.span,
                );
            }
            Declaration::TSNamespaceDeclaration(namespace) => {
                self.namespace(namespace, visibility);
            }
            _ => {}
        }
    }

    fn namespace(&mut self, namespace: &TSNamespaceDeclaration<'a>, visibility: Visibility) {
        let name = namespace.id.to_string();
        let index = self.plain(&name, SymbolKind::Module, visibility, namespace.span);
        let outer = self.parent.replace(index);
        if let TSNamespaceDeclarationBody::TSModuleBlock(block) = &namespace.body {
            self.statements(&block.body);
        }
        self.parent = outer;
    }

    fn fact(
        &self,
        name: &str,
        kind: SymbolKind,
        visibility: Visibility,
        start: u32,
        end: u32,
    ) -> SymbolFact {
        let start_line = self.lines.line_of(start);
        let end_line = self.lines.line_of(end.saturating_sub(1).max(start));
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
        }
    }

    fn plain(
        &mut self,
        name: &str,
        kind: SymbolKind,
        visibility: Visibility,
        span: oxc_span::Span,
    ) -> usize {
        let fact = self.fact(name, kind, visibility, span.start, span.end);
        self.symbols.push(fact);
        self.symbols.len() - 1
    }

    /// A variable holding a function or a class is that function or class. Other
    /// variables are only symbols at the top level, and inside a function they are part
    /// of what the function does.
    fn variables(&mut self, variables: &VariableDeclaration<'a>, visibility: Visibility) {
        for variable in &variables.declarations {
            self.variable(variable, visibility);
        }
    }

    fn variable(&mut self, variable: &VariableDeclarator<'a>, visibility: Visibility) {
        let name = binding_name(&variable.id);
        let init = variable.init.as_ref().map(Expression::get_inner_expression);
        match (name, init) {
            (Some(name), Some(Expression::ArrowFunctionExpression(arrow))) => {
                self.callable(Callable {
                    name,
                    kind: SymbolKind::Function,
                    visibility,
                    start: variable.span.start,
                    end: variable.span.end,
                    params: &arrow.params,
                    body: Some(CallableBody::Arrow(arrow)),
                });
            }
            (Some(name), Some(Expression::FunctionExpression(function))) => {
                self.callable(Callable {
                    name,
                    kind: SymbolKind::Function,
                    visibility,
                    start: variable.span.start,
                    end: variable.span.end,
                    params: &function.params,
                    body: function.body.as_deref().map(CallableBody::Function),
                });
            }
            (Some(name), Some(Expression::ClassExpression(class))) => {
                self.class(&name, class, visibility);
            }
            (name, init) => {
                if self.frame.is_none() {
                    if let Some(name) = name {
                        self.plain(&name, SymbolKind::Constant, visibility, variable.span);
                    }
                } else if let Some(init) = init {
                    self.visit_expression(init);
                }
            }
        }
    }

    fn function(&mut self, name: &str, function: &Function<'a>, visibility: Visibility) {
        let is_inside_type = self
            .parent
            .and_then(|index| self.symbols.get(index))
            .is_some_and(|parent| parent.kind.is_type());
        self.callable(Callable {
            name: name.to_owned(),
            kind: if is_inside_type {
                SymbolKind::Method
            } else {
                SymbolKind::Function
            },
            visibility,
            start: function.span.start,
            end: function.span.end,
            params: &function.params,
            body: function.body.as_deref().map(CallableBody::Function),
        });
    }

    /// Adds a function, and walks its body. An overload is not a function of its own,
    /// so the signatures and the function they describe are one symbol.
    fn callable(&mut self, callable: Callable<'_, 'a>) {
        let key = (self.parent, callable.name.clone());
        let fact = self.fact(
            &callable.name,
            callable.kind,
            callable.visibility,
            callable.start,
            callable.end,
        );
        let index = if let Some(index) = self.declared.get(&key).copied() {
            if let Some(symbol) = self.symbols.get_mut(index) {
                *symbol = fact;
            }
            index
        } else {
            self.symbols.push(fact);
            self.symbols.len() - 1
        };
        let Some(body) = callable.body else {
            self.declared.insert(key, index);
            return;
        };
        self.declared.remove(&key);

        let frame = Frame {
            name: callable.name,
            is_method: callable.kind == SymbolKind::Method,
            decisions: Vec::new(),
            max_nesting: 0,
            nesting: 0,
            depth: 0,
            recursive: false,
        };
        let outer_frame = self.frame.replace(frame);
        let outer_parent = self.parent.replace(index);
        match body {
            CallableBody::Function(body) => self.visit_function_body(body),
            CallableBody::Arrow(arrow) => self.visit_arrow_function_body(&arrow.body),
        }
        self.parent = outer_parent;
        let finished = std::mem::replace(&mut self.frame, outer_frame);

        if let (Some(mut frame), Some(symbol)) = (finished, self.symbols.get_mut(index)) {
            frame.decisions.sort_by_key(|point| point.line);
            symbol.body = Some(BodyFacts {
                decisions: frame.decisions,
                parameters: parameter_count(callable.params),
                max_nesting: frame.max_nesting,
            });
        }
    }

    fn class(&mut self, name: &str, class: &Class<'a>, visibility: Visibility) {
        let index = self.plain(name, SymbolKind::Class, visibility, class.span);
        let outer = self.parent.replace(index);
        for element in &class.body.body {
            match element {
                ClassElement::MethodDefinition(method) => self.method(method),
                ClassElement::PropertyDefinition(property) => self.property(property),
                ClassElement::AccessorProperty(property) => {
                    let name = key_name(&property.key, self.text);
                    let visibility = member_visibility(property.accessibility, &property.key);
                    self.plain(&name, SymbolKind::Field, visibility, property.span);
                }
                _ => {}
            }
        }
        self.parent = outer;
    }

    fn method(&mut self, method: &MethodDefinition<'a>) {
        let function = &method.value;
        self.parameter_properties(&function.params);
        self.callable(Callable {
            name: key_name(&method.key, self.text),
            kind: SymbolKind::Method,
            visibility: member_visibility(method.accessibility, &method.key),
            start: method.key.span().start,
            end: method.span.end,
            params: &function.params,
            body: function.body.as_deref().map(CallableBody::Function),
        });
    }

    /// `constructor(private a: number)` declares a field too.
    fn parameter_properties(&mut self, params: &FormalParameters<'_>) {
        for parameter in &params.items {
            if parameter.accessibility.is_none() && !parameter.readonly {
                continue;
            }
            if let Some(name) = binding_name(&parameter.pattern) {
                let visibility = visibility_of(parameter.accessibility);
                self.plain(&name, SymbolKind::Field, visibility, parameter.span);
            }
        }
    }

    /// A field holding a function is a method.
    fn property(&mut self, property: &PropertyDefinition<'a>) {
        let name = key_name(&property.key, self.text);
        let visibility = member_visibility(property.accessibility, &property.key);
        let value = property
            .value
            .as_ref()
            .map(Expression::get_inner_expression);
        let (params, body) = match value {
            Some(Expression::ArrowFunctionExpression(arrow)) => {
                (&arrow.params, Some(CallableBody::Arrow(arrow)))
            }
            Some(Expression::FunctionExpression(function)) => (
                &function.params,
                function.body.as_deref().map(CallableBody::Function),
            ),
            _ => {
                self.plain(&name, SymbolKind::Field, visibility, property.span);
                return;
            }
        };
        self.callable(Callable {
            name,
            kind: SymbolKind::Method,
            visibility,
            start: property.key.span().start,
            end: property.span.end,
            params,
            body,
        });
    }

    fn interface(&mut self, interface: &TSInterfaceDeclaration<'a>, visibility: Visibility) {
        let index = self.plain(
            interface.id.name.as_str(),
            SymbolKind::Interface,
            visibility,
            interface.span,
        );
        let outer = self.parent.replace(index);
        for signature in &interface.body.body {
            match signature {
                TSSignature::TSMethodSignature(method) => {
                    self.callable(Callable {
                        name: key_name(&method.key, self.text),
                        kind: SymbolKind::Method,
                        visibility: Visibility::Public,
                        start: method.span.start,
                        end: method.span.end,
                        params: &method.params,
                        body: None,
                    });
                }
                TSSignature::TSPropertySignature(property) => {
                    let name = key_name(&property.key, self.text);
                    self.plain(&name, SymbolKind::Field, Visibility::Public, property.span);
                }
                _ => {}
            }
        }
        self.parent = outer;
    }

    fn point(&mut self, kind: DecisionKind, offset: u32) {
        let line = self.lines.line_of(offset);
        if let Some(frame) = &mut self.frame {
            let nesting = frame.nesting;
            frame.decisions.push(DecisionPoint {
                kind,
                line,
                nesting,
            });
        }
    }
}

impl<'a> Extractor<'_, 'a> {
    /// Walks something one control structure deeper.
    fn deeper(&mut self, walk: impl FnOnce(&mut Self)) {
        if let Some(frame) = &mut self.frame {
            frame.nesting += 1;
            frame.depth += 1;
            frame.max_nesting = frame.max_nesting.max(frame.depth);
        }
        walk(self);
        if let Some(frame) = &mut self.frame {
            frame.nesting -= 1;
            frame.depth -= 1;
        }
    }

    /// A closure is one level deeper to read, and adds no control structure.
    fn inside_closure(&mut self, walk: impl FnOnce(&mut Self)) {
        if let Some(frame) = &mut self.frame {
            frame.nesting += 1;
        }
        walk(self);
        if let Some(frame) = &mut self.frame {
            frame.nesting -= 1;
        }
    }

    fn shortcut(&mut self, optional: bool, offset: u32) {
        if optional {
            self.point(DecisionKind::Shortcut, offset);
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

    fn calls_itself(&self, callee: &Expression<'_>) -> bool {
        let Some(frame) = &self.frame else {
            return false;
        };
        match callee.get_inner_expression() {
            Expression::Identifier(identifier) => {
                !frame.is_method && identifier.name == frame.name.as_str()
            }
            Expression::StaticMemberExpression(member) => {
                frame.is_method
                    && matches!(member.object, Expression::ThisExpression(_))
                    && member.property.name == frame.name.as_str()
            }
            _ => false,
        }
    }

    /// The loop part of `for`, `while` and the like.
    fn looping(&mut self, kind: DecisionKind, offset: u32, body: &Statement<'a>) {
        self.point(kind, offset);
        self.deeper(|walker| walker.visit_statement(body));
    }
}

/// Every operator is a branch, and each run of the same one adds a point.
fn operators<'n, 'a>(
    top: &'n LogicalExpression<'a>,
) -> (Vec<(u32, LogicalOperator)>, Vec<&'n Expression<'a>>) {
    let mut found = vec![(top.left.span().end, top.operator)];
    let mut operands = Vec::new();
    let mut pending = vec![&top.right, &top.left];
    while let Some(expression) = pending.pop() {
        match expression {
            Expression::ParenthesizedExpression(paren) => pending.push(&paren.expression),
            Expression::LogicalExpression(logical) => {
                found.push((logical.left.span().end, logical.operator));
                pending.push(&logical.right);
                pending.push(&logical.left);
            }
            other => operands.push(other),
        }
    }
    found.sort_by_key(|(offset, _)| *offset);
    (found, operands)
}

impl<'a> Visit<'a> for Extractor<'_, 'a> {
    /// Types do nothing at run time.
    fn visit_ts_type(&mut self, _: &TSType<'a>) {}

    fn visit_ts_type_annotation(&mut self, _: &TSTypeAnnotation<'a>) {}

    fn visit_statement(&mut self, statement: &Statement<'a>) {
        match statement {
            Statement::TSTypeAliasDeclaration(_)
            | Statement::TSInterfaceDeclaration(_)
            | Statement::TSEnumDeclaration(_) => {
                if let Some(declaration) = statement.as_declaration() {
                    self.declaration(declaration, Visibility::Private);
                }
            }
            other => walk::walk_statement(self, other),
        }
    }

    fn visit_variable_declarator(&mut self, variable: &VariableDeclarator<'a>) {
        self.variable(variable, Visibility::Private);
    }

    fn visit_function(&mut self, function: &Function<'a>, flags: ScopeFlags) {
        let named_declaration = function.r#type == FunctionType::FunctionDeclaration;
        match &function.id {
            Some(id) if named_declaration => {
                self.function(id.name.as_str(), function, Visibility::Private);
            }
            _ => self.inside_closure(|walker| walk::walk_function(walker, function, flags)),
        }
    }

    fn visit_arrow_function_expression(&mut self, arrow: &ArrowFunctionExpression<'a>) {
        self.inside_closure(|walker| walk::walk_arrow_function_expression(walker, arrow));
    }

    fn visit_class(&mut self, class: &Class<'a>) {
        if let Some(id) = &class.id {
            self.class(id.name.as_str(), class, Visibility::Private);
        }
    }

    fn visit_if_statement(&mut self, statement: &IfStatement<'a>) {
        self.point(DecisionKind::If, statement.span.start);
        let mut current = statement;
        loop {
            self.visit_expression(&current.test);
            self.deeper(|walker| walker.visit_statement(&current.consequent));
            match &current.alternate {
                Some(Statement::IfStatement(next)) => {
                    self.point(DecisionKind::ElseIf, next.span.start);
                    current = next;
                }
                Some(other) => {
                    self.point(DecisionKind::Else, other.span().start);
                    self.deeper(|walker| walker.visit_statement(other));
                    return;
                }
                None => return,
            }
        }
    }

    fn visit_for_statement(&mut self, statement: &ForStatement<'a>) {
        if let Some(init) = &statement.init {
            self.visit_for_statement_init(init);
        }
        let kind = match &statement.test {
            Some(test) => {
                self.visit_expression(test);
                DecisionKind::Loop
            }
            None => DecisionKind::BareLoop,
        };
        if let Some(update) = &statement.update {
            self.visit_expression(update);
        }
        self.looping(kind, statement.span.start, &statement.body);
    }

    fn visit_for_of_statement(&mut self, statement: &ForOfStatement<'a>) {
        self.visit_expression(&statement.right);
        self.looping(DecisionKind::Loop, statement.span.start, &statement.body);
    }

    fn visit_for_in_statement(&mut self, statement: &ForInStatement<'a>) {
        self.visit_expression(&statement.right);
        self.looping(DecisionKind::Loop, statement.span.start, &statement.body);
    }

    fn visit_while_statement(&mut self, statement: &WhileStatement<'a>) {
        self.visit_expression(&statement.test);
        self.looping(DecisionKind::Loop, statement.span.start, &statement.body);
    }

    fn visit_do_while_statement(&mut self, statement: &DoWhileStatement<'a>) {
        self.visit_expression(&statement.test);
        self.looping(DecisionKind::Loop, statement.span.start, &statement.body);
    }

    fn visit_switch_statement(&mut self, statement: &SwitchStatement<'a>) {
        self.point(DecisionKind::Match, statement.span.start);
        self.visit_expression(&statement.discriminant);
        self.deeper(|walker| {
            for case in &statement.cases {
                if let Some(test) = &case.test {
                    walker.point(DecisionKind::MatchArm, case.span.start);
                    walker.visit_expression(test);
                }
                for inner in &case.consequent {
                    walker.visit_statement(inner);
                }
            }
        });
    }

    fn visit_try_statement(&mut self, statement: &TryStatement<'a>) {
        self.visit_block_statement(&statement.block);
        if let Some(handler) = &statement.handler {
            self.visit_catch_clause(handler);
        }
        if let Some(finalizer) = &statement.finalizer {
            self.visit_block_statement(finalizer);
        }
    }

    fn visit_catch_clause(&mut self, clause: &CatchClause<'a>) {
        self.point(DecisionKind::Catch, clause.span.start);
        self.deeper(|walker| walker.visit_block_statement(&clause.body));
    }

    fn visit_conditional_expression(&mut self, expression: &ConditionalExpression<'a>) {
        self.point(DecisionKind::Ternary, expression.span.start);
        self.visit_expression(&expression.test);
        self.deeper(|walker| {
            walker.visit_expression(&expression.consequent);
            walker.visit_expression(&expression.alternate);
        });
    }

    fn visit_logical_expression(&mut self, expression: &LogicalExpression<'a>) {
        let (found, operands) = operators(expression);
        let mut previous = None;
        for (offset, operator) in found {
            self.point(DecisionKind::LogicalOperator, offset);
            if previous != Some(operator) {
                self.point(DecisionKind::LogicalRun, offset);
            }
            previous = Some(operator);
        }
        for operand in operands {
            self.visit_expression(operand);
        }
    }

    /// `a ||= b` and the like skip code too.
    fn visit_assignment_expression(&mut self, expression: &AssignmentExpression<'a>) {
        if expression.operator.is_logical() {
            self.point(DecisionKind::LogicalOperator, expression.span.start);
            self.point(DecisionKind::LogicalRun, expression.span.start);
        }
        walk::walk_assignment_expression(self, expression);
    }

    fn visit_call_expression(&mut self, expression: &CallExpression<'a>) {
        self.shortcut(expression.optional, expression.span.start);
        if self.calls_itself(&expression.callee) {
            self.recursion(expression.span.start);
        }
        walk::walk_call_expression(self, expression);
    }

    fn visit_static_member_expression(&mut self, expression: &StaticMemberExpression<'a>) {
        self.shortcut(expression.optional, expression.span.start);
        walk::walk_static_member_expression(self, expression);
    }

    fn visit_computed_member_expression(&mut self, expression: &ComputedMemberExpression<'a>) {
        self.shortcut(expression.optional, expression.span.start);
        walk::walk_computed_member_expression(self, expression);
    }

    fn visit_private_field_expression(&mut self, expression: &PrivateFieldExpression<'a>) {
        self.shortcut(expression.optional, expression.span.start);
        walk::walk_private_field_expression(self, expression);
    }

    fn visit_break_statement(&mut self, statement: &BreakStatement<'a>) {
        if statement.label.is_some() {
            self.point(DecisionKind::LabeledJump, statement.span.start);
        }
    }

    fn visit_continue_statement(&mut self, statement: &ContinueStatement<'a>) {
        if statement.label.is_some() {
            self.point(DecisionKind::LabeledJump, statement.span.start);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::extract::extract;
    use cyclops_domain::facts::{BodyFacts, DecisionKind, FileFacts, SymbolFact};
    use cyclops_domain::path::RelativePath;
    use cyclops_domain::symbol::{SymbolKind, Visibility};

    fn facts(text: &str) -> FileFacts {
        let facts = extract(&RelativePath::new("a.ts"), text);
        assert!(!facts.syntax_error, "{text}");
        facts
    }

    fn symbol<'a>(facts: &'a FileFacts, name: &str) -> &'a SymbolFact {
        facts
            .symbols
            .iter()
            .find(|symbol| symbol.name == name)
            .expect("the symbol exists")
    }

    fn names(facts: &FileFacts) -> Vec<&str> {
        facts
            .symbols
            .iter()
            .map(|symbol| symbol.name.as_str())
            .collect()
    }

    fn body<'a>(facts: &'a FileFacts, name: &str) -> &'a BodyFacts {
        symbol(facts, name).body.as_ref().expect("a body")
    }

    fn kinds(facts: &FileFacts, name: &str) -> Vec<DecisionKind> {
        body(facts, name)
            .decisions
            .iter()
            .map(|point| point.kind)
            .collect()
    }

    #[test]
    fn overloads_are_one_function() {
        let facts = facts(
            "export function f(a: string): void;\nexport function f(a: number): void;\nexport function f(a: any) {\n  if (a) {}\n}\n",
        );
        assert_eq!(names(&facts), ["f"]);
        assert_eq!(kinds(&facts, "f"), [DecisionKind::If]);
    }

    #[test]
    fn overloaded_methods_are_one_method() {
        let facts = facts("class C {\n  x(a: string): void;\n  x(a: any) {}\n}\n");
        assert_eq!(names(&facts), ["C", "x"]);
        assert!(symbol(&facts, "x").body.is_some());
    }

    #[test]
    fn exporting_by_name_makes_a_symbol_public() {
        let facts =
            facts("function a() {}\nconst b = 1;\nfunction c() {}\nexport { a, b as d };\n");
        assert_eq!(symbol(&facts, "a").visibility, Visibility::Public);
        assert_eq!(symbol(&facts, "b").visibility, Visibility::Public);
        assert_eq!(symbol(&facts, "c").visibility, Visibility::Private);
    }

    #[test]
    fn a_default_export_is_public_with_or_without_a_name() {
        let named = facts("export default function f() {}\n");
        assert_eq!(symbol(&named, "f").visibility, Visibility::Public);
        let anonymous = facts("export default () => {};\n");
        assert_eq!(symbol(&anonymous, "default").kind, SymbolKind::Function);
        let later = facts("function g() {}\nexport default g;\n");
        assert_eq!(symbol(&later, "g").visibility, Visibility::Public);
    }

    #[test]
    fn a_re_export_adds_no_symbols() {
        assert_eq!(
            names(&facts("export { a } from './b';\nexport * from './c';\n")),
            [""; 0]
        );
    }

    #[test]
    fn a_parameter_property_is_a_field() {
        let facts = facts(
            "class C {\n  constructor(private a: number, readonly b: string, c: number) {}\n}\n",
        );
        assert_eq!(symbol(&facts, "a").kind, SymbolKind::Field);
        assert_eq!(symbol(&facts, "a").visibility, Visibility::Private);
        assert_eq!(symbol(&facts, "b").visibility, Visibility::Public);
        assert!(!names(&facts).contains(&"c"));
        assert_eq!(body(&facts, "constructor").parameters, 3);
    }

    #[test]
    fn the_members_of_an_interface_are_public_and_have_no_body() {
        let facts = facts("interface I {\n  x(): void;\n  y: number;\n}\n");
        assert_eq!(symbol(&facts, "x").kind, SymbolKind::Method);
        assert_eq!(symbol(&facts, "x").body, None);
        assert_eq!(symbol(&facts, "y").kind, SymbolKind::Field);
        assert_eq!(symbol(&facts, "y").visibility, Visibility::Public);
    }

    #[test]
    fn a_field_holding_a_function_is_a_method() {
        let facts = facts("class C {\n  x = (a: boolean) => {\n    if (a) {}\n  };\n}\n");
        assert_eq!(symbol(&facts, "x").kind, SymbolKind::Method);
        assert_eq!(kinds(&facts, "x"), [DecisionKind::If]);
        assert_eq!(symbol(&facts, "x").code_lines, 3);
    }

    #[test]
    fn a_namespace_is_a_module() {
        let facts = facts("namespace N {\n  export function f() {}\n}\n");
        assert_eq!(symbol(&facts, "N").kind, SymbolKind::Module);
        assert_eq!(symbol(&facts, "f").parent, Some(0));
    }

    #[test]
    fn a_class_held_in_a_variable_is_a_class() {
        assert_eq!(
            symbol(&facts("const C = class {};\n"), "C").kind,
            SymbolKind::Class
        );
    }

    #[test]
    fn a_function_held_in_a_local_variable_is_measured_on_its_own() {
        let facts =
            facts("function f(a: boolean) {\n  const g = () => {\n    if (a) {}\n  };\n}\n");
        assert_eq!(symbol(&facts, "g").parent, Some(0));
        assert_eq!(kinds(&facts, "f"), []);
        assert_eq!(kinds(&facts, "g"), [DecisionKind::If]);
    }

    #[test]
    fn a_local_variable_is_not_a_symbol_and_what_it_holds_still_counts() {
        let facts = facts("function f(a: boolean) {\n  const b = a ? 1 : 2;\n}\n");
        assert_eq!(names(&facts), ["f"]);
        assert_eq!(kinds(&facts, "f"), [DecisionKind::Ternary]);
    }

    #[test]
    fn a_class_inside_a_function_keeps_its_own_methods() {
        let facts = facts(
            "function f() {\n  class C {\n    x(a: boolean) {\n      if (a) {}\n    }\n  }\n}\n",
        );
        assert_eq!(symbol(&facts, "C").parent, Some(0));
        assert_eq!(kinds(&facts, "f"), []);
        assert_eq!(kinds(&facts, "x"), [DecisionKind::If]);
    }

    #[test]
    fn each_step_of_an_optional_chain_is_a_branch() {
        let facts = facts("function f(a: any) {\n  return a?.b?.[1]?.();\n}\n");
        assert_eq!(kinds(&facts, "f"), [DecisionKind::Shortcut; 3]);
    }

    #[test]
    fn a_logical_assignment_is_an_operator() {
        let facts = facts("function f(a: any) {\n  a ??= 1;\n}\n");
        assert_eq!(
            kinds(&facts, "f"),
            [DecisionKind::LogicalOperator, DecisionKind::LogicalRun]
        );
    }

    #[test]
    fn parentheses_do_not_break_a_run_of_operators() {
        let facts =
            facts("function f(a: boolean, b: boolean, c: boolean) {\n  return (a && b) && c;\n}\n");
        let runs = kinds(&facts, "f")
            .into_iter()
            .filter(|kind| *kind == DecisionKind::LogicalRun);
        assert_eq!(runs.count(), 1);
    }

    #[test]
    fn what_is_inside_an_operand_is_still_walked() {
        let facts = facts("function f(a: boolean) {\n  return a && [1].some(() => a || a);\n}\n");
        let runs = kinds(&facts, "f")
            .into_iter()
            .filter(|kind| *kind == DecisionKind::LogicalRun);
        assert_eq!(runs.count(), 2);
    }

    #[test]
    fn a_for_with_no_test_is_a_bare_loop() {
        let facts = facts("function f() {\n  for (;;) {}\n}\n");
        assert_eq!(kinds(&facts, "f"), [DecisionKind::BareLoop]);
    }

    #[test]
    fn a_method_recurses_through_this_and_a_function_does_not() {
        let method = facts("class C {\n  x() {\n    this.x();\n    this.x();\n  }\n}\n");
        assert_eq!(kinds(&method, "x"), [DecisionKind::Recursion]);
        let function = facts("function x() {\n  this.x();\n}\n");
        assert_eq!(kinds(&function, "x"), []);
    }

    #[test]
    fn types_add_no_complexity() {
        let facts = facts("function f(a: string extends number ? 1 : 2): void {}\n");
        assert_eq!(kinds(&facts, "f"), []);
    }

    #[test]
    fn nesting_is_tracked_through_closures_without_adding_depth() {
        let facts = facts(
            "function f(a: boolean) {\n  if (a) {\n    [1].map(() => {\n      if (a) {}\n    });\n  }\n}\n",
        );
        let body = body(&facts, "f");
        assert_eq!(body.max_nesting, 2);
        let nesting: Vec<u32> = body.decisions.iter().map(|point| point.nesting).collect();
        assert_eq!(nesting, [0, 2]);
    }

    #[test]
    fn a_computed_key_keeps_its_written_name() {
        let facts = facts("class C {\n  [Symbol.iterator]() {}\n}\n");
        assert_eq!(symbol(&facts, "Symbol.iterator").kind, SymbolKind::Method);
    }
}

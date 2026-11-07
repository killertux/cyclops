//! Walks the body of a function and describes what makes it complex. It keeps its own
//! stack of work rather than recursing, so that very deep code cannot overflow.

use cyclops_domain::facts::{BodyFacts, DecisionKind, DecisionPoint, Reference, ReferenceKind};
use cyclops_domain::lines::LineMap;
use ra_ap_syntax::ast::{self, AstNode, BinaryOp, ElseBranch, HasName, LogicOp};
use ra_ap_syntax::{SyntaxKind, SyntaxNode, SyntaxToken, TextSize};

/// The function being walked, for spotting recursion.
pub(crate) struct Function<'a> {
    pub(crate) name: &'a str,
    pub(crate) is_method: bool,
}

/// What the walk found.
pub(crate) struct Walked {
    pub(crate) body: BodyFacts,
    /// Items written inside the function. They are measured on their own.
    pub(crate) items: Vec<ast::Item>,
    /// What the body mentions and calls.
    pub(crate) references: Vec<Reference>,
}

/// The names of a path, such as `crate`, `billing` and `Invoice`, without generic
/// arguments.
pub(crate) fn path_names(path: &ast::Path) -> Vec<String> {
    path.segments()
        .map(|segment| {
            segment.name_ref().map_or_else(
                || segment.syntax().text().to_string(),
                |name| name.text().to_string(),
            )
        })
        .collect()
}

/// Every type a piece of syntax names, such as the parameters of a function.
pub(crate) fn type_references(node: &SyntaxNode, kind: ReferenceKind) -> Vec<Reference> {
    node.descendants()
        .filter_map(ast::PathType::cast)
        .filter_map(|ty| ty.path())
        .map(|path| Reference {
            kind,
            path: path_names(&path),
        })
        .collect()
}

fn is_self(expr: Option<ast::Expr>) -> bool {
    expr.is_some_and(|expr| expr.syntax().text() == "self")
}

/// A part of the code still to walk.
struct Step {
    node: SyntaxNode,
    /// How deep it is for cognitive complexity, which also counts closures.
    nesting: u32,
    /// How many control structures are around it.
    depth: u32,
}

struct Walker<'a> {
    lines: &'a LineMap,
    function: Function<'a>,
    steps: Vec<Step>,
    decisions: Vec<DecisionPoint>,
    max_nesting: u32,
    items: Vec<ast::Item>,
    recursive: bool,
    references: Vec<Reference>,
    fields_used: Vec<String>,
    methods_called: Vec<String>,
}

pub(crate) fn walk(lines: &LineMap, function: Function<'_>, body: &ast::BlockExpr) -> Walked {
    let mut walker = Walker {
        lines,
        function,
        steps: Vec::new(),
        decisions: Vec::new(),
        max_nesting: 0,
        items: Vec::new(),
        recursive: false,
        references: Vec::new(),
        fields_used: Vec::new(),
        methods_called: Vec::new(),
    };
    walker.push(Some(body.syntax()), 0, 0);
    while let Some(step) = walker.steps.pop() {
        walker.visit(step);
    }
    walker.decisions.sort_by_key(|point| point.line);
    walker.fields_used.sort();
    walker.fields_used.dedup();
    walker.methods_called.sort();
    walker.methods_called.dedup();
    Walked {
        body: BodyFacts {
            decisions: walker.decisions,
            parameters: 0,
            max_nesting: walker.max_nesting,
            fields_used: walker.fields_used,
            methods_called: walker.methods_called,
        },
        items: walker.items,
        references: walker.references,
    }
}

fn is_item(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::FN
            | SyntaxKind::STRUCT
            | SyntaxKind::ENUM
            | SyntaxKind::UNION
            | SyntaxKind::TRAIT
            | SyntaxKind::TYPE_ALIAS
            | SyntaxKind::CONST
            | SyntaxKind::STATIC
            | SyntaxKind::IMPL
            | SyntaxKind::MODULE
            | SyntaxKind::USE
    )
}

fn logical_op(expr: &ast::BinExpr) -> Option<LogicOp> {
    match expr.op_kind()? {
        BinaryOp::LogicOp(op) => Some(op),
        _ => None,
    }
}

/// The logical expression an operand belongs to, looking through parentheses.
fn enclosing_logical(expr: &ast::BinExpr) -> Option<ast::BinExpr> {
    let mut parent = expr.syntax().parent();
    while let Some(node) = parent {
        if node.kind() != SyntaxKind::PAREN_EXPR {
            return ast::BinExpr::cast(node).filter(|outer| logical_op(outer).is_some());
        }
        parent = node.parent();
    }
    None
}

/// The operators of a whole logical expression, in the order they are written.
fn operators(top: &ast::BinExpr) -> Vec<(TextSize, LogicOp, Option<SyntaxToken>)> {
    let mut found = Vec::new();
    let mut pending = vec![ast::Expr::BinExpr(top.clone())];
    while let Some(expr) = pending.pop() {
        match expr {
            ast::Expr::ParenExpr(paren) => pending.extend(paren.expr()),
            ast::Expr::BinExpr(bin) => {
                if let Some(op) = logical_op(&bin) {
                    let token = bin.op_token();
                    let offset = token.as_ref().map_or_else(
                        || bin.syntax().text_range().start(),
                        |t| t.text_range().start(),
                    );
                    found.push((offset, op, token));
                    pending.extend(bin.lhs());
                    pending.extend(bin.rhs());
                }
            }
            _ => {}
        }
    }
    found.sort_by_key(|(offset, ..)| *offset);
    found
}

impl Walker<'_> {
    fn reference(&mut self, kind: ReferenceKind, path: Vec<String>) {
        if !path.is_empty() {
            self.references.push(Reference { kind, path });
        }
    }

    /// Types named in the body: in annotations, in struct literals, and before `::`.
    fn mentions(&mut self, node: &SyntaxNode) {
        if let Some(ty) = ast::PathType::cast(node.clone()) {
            if let Some(path) = ty.path() {
                self.reference(ReferenceKind::Type, path_names(&path));
            }
        } else if let Some(record) = ast::RecordExpr::cast(node.clone()) {
            if let Some(path) = record.path() {
                self.reference(ReferenceKind::Type, path_names(&path));
            }
        } else if let Some(expr) = ast::PathExpr::cast(node.clone())
            && let Some(path) = expr.path()
        {
            let mut names = path_names(&path);
            names.pop();
            let only_keywords = names
                .iter()
                .all(|name| matches!(name.as_str(), "self" | "super" | "crate" | "Self"));
            if !only_keywords {
                self.reference(ReferenceKind::Type, names);
            }
        }
    }

    fn push(&mut self, node: Option<&SyntaxNode>, nesting: u32, depth: u32) {
        if let Some(node) = node {
            self.steps.push(Step {
                node: node.clone(),
                nesting,
                depth,
            });
        }
    }

    fn point(&mut self, kind: DecisionKind, offset: TextSize, nesting: u32) {
        self.decisions.push(DecisionPoint {
            kind,
            line: self.lines.line_of(offset.into()),
            nesting,
        });
    }

    fn at(node: &SyntaxNode) -> TextSize {
        node.text_range().start()
    }

    /// Goes one control structure deeper, and returns the new depth.
    fn deeper(&mut self, depth: u32) -> u32 {
        self.max_nesting = self.max_nesting.max(depth + 1);
        depth + 1
    }

    fn visit(&mut self, step: Step) {
        let Step {
            node,
            nesting,
            depth,
        } = step;
        if is_item(node.kind()) {
            self.items.extend(ast::Item::cast(node));
            return;
        }
        self.mentions(&node);
        let known = ast::Expr::cast(node.clone())
            .is_some_and(|expr| self.visit_expr(&expr, nesting, depth));
        if known {
            return;
        }
        if let Some(stmt) = ast::LetStmt::cast(node.clone())
            && let Some(let_else) = stmt.let_else()
        {
            self.visit_let_else(&stmt, &let_else, nesting, depth);
            return;
        }
        for child in node.children() {
            self.push(Some(&child), nesting, depth);
        }
    }

    /// Handles what needs more than walking its children, and says if it did.
    fn visit_expr(&mut self, expr: &ast::Expr, nesting: u32, depth: u32) -> bool {
        match expr {
            ast::Expr::IfExpr(expr) => self.visit_if(expr, nesting, depth),
            ast::Expr::ForExpr(expr) => {
                self.visit_loop(DecisionKind::Loop, expr.syntax(), nesting, depth);
            }
            ast::Expr::WhileExpr(expr) => {
                self.visit_loop(DecisionKind::Loop, expr.syntax(), nesting, depth);
            }
            ast::Expr::LoopExpr(expr) => {
                self.visit_loop(DecisionKind::BareLoop, expr.syntax(), nesting, depth);
            }
            ast::Expr::MatchExpr(expr) => self.visit_match(expr, nesting, depth),
            ast::Expr::ClosureExpr(expr) => {
                let body = expr.body();
                self.push(body.as_ref().map(AstNode::syntax), nesting + 1, depth);
            }
            ast::Expr::BreakExpr(expr) if expr.lifetime().is_some() => {
                self.point(DecisionKind::LabeledJump, Self::at(expr.syntax()), nesting);
                return false;
            }
            ast::Expr::ContinueExpr(expr) if expr.lifetime().is_some() => {
                self.point(DecisionKind::LabeledJump, Self::at(expr.syntax()), nesting);
                return false;
            }
            ast::Expr::TryExpr(expr) => {
                let offset = expr.question_mark_token().map_or_else(
                    || Self::at(expr.syntax()),
                    |token| token.text_range().start(),
                );
                self.point(DecisionKind::Shortcut, offset, nesting);
                return false;
            }
            ast::Expr::BinExpr(expr) => {
                self.visit_logical(expr, nesting);
                return false;
            }
            ast::Expr::CallExpr(expr) => {
                self.visit_call(expr, nesting);
                return false;
            }
            ast::Expr::MethodCallExpr(expr) => {
                self.visit_method_call(expr, nesting);
                return false;
            }
            ast::Expr::FieldExpr(expr) => {
                if is_self(expr.expr())
                    && let Some(name) = expr.name_ref()
                {
                    self.fields_used.push(name.text().to_string());
                }
                return false;
            }
            _ => return false,
        }
        true
    }

    fn visit_if(&mut self, expr: &ast::IfExpr, nesting: u32, depth: u32) {
        self.point(DecisionKind::If, Self::at(expr.syntax()), nesting);
        let inner = self.deeper(depth);
        let mut current = expr.clone();
        loop {
            let condition = current.condition();
            self.push(condition.as_ref().map(AstNode::syntax), nesting, depth);
            let then_branch = current.then_branch();
            self.push(
                then_branch.as_ref().map(AstNode::syntax),
                nesting + 1,
                inner,
            );

            let offset = current.else_token().map_or_else(
                || Self::at(current.syntax()),
                |token| token.text_range().start(),
            );
            match current.else_branch() {
                Some(ElseBranch::IfExpr(next)) => {
                    self.point(DecisionKind::ElseIf, offset, nesting);
                    current = next;
                }
                Some(ElseBranch::Block(block)) => {
                    self.point(DecisionKind::Else, offset, nesting);
                    self.push(Some(block.syntax()), nesting + 1, inner);
                    return;
                }
                None => return,
            }
        }
    }

    fn visit_let_else(
        &mut self,
        stmt: &ast::LetStmt,
        let_else: &ast::LetElse,
        nesting: u32,
        depth: u32,
    ) {
        self.point(DecisionKind::If, Self::at(let_else.syntax()), nesting);
        let inner = self.deeper(depth);
        let initializer = stmt.initializer();
        self.push(initializer.as_ref().map(AstNode::syntax), nesting, depth);
        let block = let_else.block_expr();
        self.push(block.as_ref().map(AstNode::syntax), nesting + 1, inner);
    }

    /// The header of a loop is outside it, and its body is one level in.
    fn visit_loop(&mut self, kind: DecisionKind, node: &SyntaxNode, nesting: u32, depth: u32) {
        self.point(kind, Self::at(node), nesting);
        let inner = self.deeper(depth);
        for child in node.children() {
            if child.kind() == SyntaxKind::BLOCK_EXPR {
                self.push(Some(&child), nesting + 1, inner);
            } else {
                self.push(Some(&child), nesting, depth);
            }
        }
    }

    fn visit_match(&mut self, expr: &ast::MatchExpr, nesting: u32, depth: u32) {
        self.point(DecisionKind::Match, Self::at(expr.syntax()), nesting);
        let inner = self.deeper(depth);
        let scrutinee = expr.expr();
        self.push(scrutinee.as_ref().map(AstNode::syntax), nesting, depth);
        let arms = expr
            .match_arm_list()
            .into_iter()
            .flat_map(|list| list.arms());
        for (index, arm) in arms.enumerate() {
            if index > 0 {
                self.point(DecisionKind::MatchArm, Self::at(arm.syntax()), nesting);
            }
            if let Some(guard) = arm.guard() {
                self.point(DecisionKind::Guard, Self::at(guard.syntax()), nesting);
                self.push(Some(guard.syntax()), nesting + 1, inner);
            }
            let body = arm.expr();
            self.push(body.as_ref().map(AstNode::syntax), nesting + 1, inner);
        }
    }

    /// Every operator is a branch. Only the top of an expression counts its runs.
    fn visit_logical(&mut self, expr: &ast::BinExpr, nesting: u32) {
        if logical_op(expr).is_none() {
            return;
        }
        let token = expr.op_token();
        let offset = token.map_or_else(|| Self::at(expr.syntax()), |t| t.text_range().start());
        self.point(DecisionKind::LogicalOperator, offset, nesting);
        if enclosing_logical(expr).is_some() {
            return;
        }
        let mut previous = None;
        for (offset, op, _) in operators(expr) {
            if previous != Some(op) {
                self.point(DecisionKind::LogicalRun, offset, nesting);
            }
            previous = Some(op);
        }
    }

    fn visit_call(&mut self, expr: &ast::CallExpr, nesting: u32) {
        let Some(ast::Expr::PathExpr(callee)) = expr.expr() else {
            return;
        };
        if let Some(path) = callee.path() {
            let names = path_names(&path);
            match names.as_slice() {
                [first, name] if first == "Self" => {
                    self.reference(ReferenceKind::SelfCall, vec![name.clone()]);
                }
                _ => self.reference(ReferenceKind::Call, names),
            }
        }
        let path = callee.syntax().text().to_string();
        let name = self.function.name;
        let calls_itself = if self.function.is_method {
            path.strip_prefix("Self::") == Some(name)
        } else {
            path == name
        };
        if calls_itself {
            self.recursion(expr.syntax(), nesting);
        }
    }

    fn visit_method_call(&mut self, expr: &ast::MethodCallExpr, nesting: u32) {
        let on_self = is_self(expr.receiver());
        if let Some(name) = expr.name_ref().map(|name| name.text().to_string()) {
            if on_self {
                self.methods_called.push(name.clone());
                self.reference(ReferenceKind::SelfCall, vec![name]);
            } else {
                self.reference(ReferenceKind::MethodCall, vec![name]);
            }
        }
        let same_name = expr
            .name_ref()
            .is_some_and(|name| name.text() == self.function.name);
        if self.function.is_method && on_self && same_name {
            self.recursion(expr.syntax(), nesting);
        }
    }

    /// A function is only charged once for calling itself.
    fn recursion(&mut self, node: &SyntaxNode, nesting: u32) {
        if !self.recursive {
            self.recursive = true;
            self.point(DecisionKind::Recursion, Self::at(node), nesting);
        }
    }
}

/// The name a function goes by.
pub(crate) fn function_name(function: &ast::Fn) -> Option<String> {
    function.name().map(|name| name.text().to_string())
}

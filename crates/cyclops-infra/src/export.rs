//! Writes diagrams in formats other tools read: Mermaid, DOT and `PlantUML`. At type level
//! each is a UML class diagram.

use cyclops_application::diagram::{DiagramView, Edge, Level, Member, Node};
use cyclops_domain::metric::MetricId;
use cyclops_domain::relation::{Confidence, RelationKind};
use cyclops_domain::symbol::{SymbolId, Visibility};
use std::fmt::{self, Write};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Mermaid,
    Dot,
    PlantUml,
}

impl ExportFormat {
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "mermaid" => Some(Self::Mermaid),
            "dot" => Some(Self::Dot),
            "plantuml" => Some(Self::PlantUml),
            _ => None,
        }
    }
}

/// A diagram to write, with what it shows besides its nodes and edges.
#[derive(Debug, Clone, Copy)]
pub struct Export<'a> {
    pub view: &'a DiagramView,
    pub level: Level,
    pub overlay: Option<MetricId>,
}

const fn visibility(visibility: Visibility) -> char {
    match visibility {
        Visibility::Public => '+',
        Visibility::Restricted => '~',
        Visibility::Private => '-',
    }
}

/// An edge's label, which says when it is only a guess, so it stays one elsewhere too.
fn label(edge: &Edge) -> String {
    match edge.confidence {
        Confidence::Exact => edge.kind.name().to_owned(),
        other => format!("{} ({})", edge.kind.name(), other.name()),
    }
}

fn score(overlay: Option<MetricId>, value: Option<f64>) -> String {
    match (overlay, value) {
        (Some(metric), Some(value)) => format!(" {metric} {}", metric.descriptor().format(value)),
        _ => String::new(),
    }
}

fn node_label(node: &Node, overlay: Option<MetricId>) -> String {
    format!("{}{}", node.id, score(overlay, node.overlay))
}

fn member_line(member: &Member, method: bool, overlay: Option<MetricId>) -> String {
    let call = if method { "()" } else { "" };
    format!(
        "{}{}{call}{}",
        visibility(member.visibility),
        member.name,
        score(overlay, member.overlay)
    )
}

/// Names that every format accepts, one for each node.
fn names(view: &DiagramView) -> Vec<(String, &Node)> {
    view.nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (format!("n{index}"), node))
        .collect()
}

fn name_of<'a>(names: &'a [(String, &Node)], id: &SymbolId) -> Option<&'a str> {
    names
        .iter()
        .find(|(_, node)| &node.id == id)
        .map(|(name, _)| name.as_str())
}

fn quoted(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

/// The UML arrow for a relation, from `from` to `to`.
fn arrow(kind: RelationKind, from: &str, to: &str) -> String {
    match kind {
        RelationKind::Inherits => format!("{to} <|-- {from}"),
        RelationKind::Implements => format!("{to} <|.. {from}"),
        RelationKind::Composes => format!("{from} *-- {to}"),
        _ => format!("{from} ..> {to}"),
    }
}

impl Export<'_> {
    /// The diagram in a format.
    ///
    /// # Errors
    ///
    /// Fails only if text cannot be written, which does not happen in memory.
    pub fn write(&self, format: ExportFormat) -> Result<String, fmt::Error> {
        let mut out = String::new();
        match (format, self.level) {
            (ExportFormat::Mermaid, Level::Type) => self.mermaid_classes(&mut out)?,
            (ExportFormat::Mermaid, _) => self.mermaid_graph(&mut out)?,
            (ExportFormat::Dot, _) => self.dot(&mut out)?,
            (ExportFormat::PlantUml, _) => self.plantuml(&mut out)?,
        }
        Ok(out)
    }

    fn edges<'v>(
        &'v self,
        names: &'v [(String, &Node)],
    ) -> impl Iterator<Item = (&'v Edge, &'v str, &'v str)> {
        self.view.edges.iter().filter_map(|edge| {
            let from = name_of(names, &edge.from)?;
            let to = name_of(names, &edge.to)?;
            Some((edge, from, to))
        })
    }

    fn mermaid_graph(&self, out: &mut String) -> fmt::Result {
        let names = names(self.view);
        writeln!(out, "graph LR")?;
        for (name, node) in &names {
            writeln!(
                out,
                "  {name}[\"{}\"]",
                quoted(&node_label(node, self.overlay))
            )?;
        }
        for (edge, from, to) in self.edges(&names) {
            writeln!(out, "  {from} -->|{}| {to}", label(edge))?;
        }
        Ok(())
    }

    fn mermaid_classes(&self, out: &mut String) -> fmt::Result {
        let names = names(self.view);
        writeln!(out, "classDiagram")?;
        for (name, node) in &names {
            writeln!(
                out,
                "  class {name}[\"{}\"] {{",
                quoted(&node_label(node, self.overlay))
            )?;
            if let Some(stereotype) = node.stereotype {
                writeln!(out, "    <<{stereotype}>>")?;
            }
            for field in &node.fields {
                writeln!(out, "    {}", member_line(field, false, self.overlay))?;
            }
            for method in &node.methods {
                writeln!(out, "    {}", member_line(method, true, self.overlay))?;
            }
            writeln!(out, "  }}")?;
        }
        for (edge, from, to) in self.edges(&names) {
            writeln!(out, "  {} : {}", arrow(edge.kind, from, to), label(edge))?;
        }
        Ok(())
    }

    fn dot(&self, out: &mut String) -> fmt::Result {
        let names = names(self.view);
        writeln!(out, "digraph cyclops {{")?;
        writeln!(out, "  node [shape=box];")?;
        for (name, node) in &names {
            let mut lines = vec![node_label(node, self.overlay)];
            lines.extend(
                node.fields
                    .iter()
                    .map(|field| member_line(field, false, self.overlay)),
            );
            lines.extend(
                node.methods
                    .iter()
                    .map(|method| member_line(method, true, self.overlay)),
            );
            let text: Vec<String> = lines.iter().map(|line| quoted(line)).collect();
            writeln!(out, "  {name} [label=\"{}\"];", text.join("\\n"))?;
        }
        for (edge, from, to) in self.edges(&names) {
            writeln!(
                out,
                "  {from} -> {to} [label=\"{}\"];",
                quoted(&label(edge))
            )?;
        }
        writeln!(out, "}}")
    }

    fn plantuml(&self, out: &mut String) -> fmt::Result {
        let names = names(self.view);
        writeln!(out, "@startuml")?;
        for (name, node) in &names {
            let keyword = match node.stereotype {
                Some("interface") => "interface",
                Some("enum") => "enum",
                _ if self.level == Level::Type => "class",
                _ => "component",
            };
            write!(
                out,
                "{keyword} \"{}\" as {name}",
                quoted(&node_label(node, self.overlay))
            )?;
            if node.fields.is_empty() && node.methods.is_empty() {
                writeln!(out)?;
                continue;
            }
            writeln!(out, " {{")?;
            for field in &node.fields {
                writeln!(out, "  {}", member_line(field, false, self.overlay))?;
            }
            for method in &node.methods {
                writeln!(out, "  {}", member_line(method, true, self.overlay))?;
            }
            writeln!(out, "}}")?;
        }
        for (edge, from, to) in self.edges(&names) {
            writeln!(out, "{} : {}", arrow(edge.kind, from, to), label(edge))?;
        }
        writeln!(out, "@enduml")
    }
}

use std::collections::{HashMap, HashSet};

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use filament_core::models::{Entity, EntityStatus, Relation, RelationType};

/// Data needed to render the graph view.
#[derive(Default)]
pub struct GraphData {
    pub entities: Vec<Entity>,
    pub relations: Vec<Relation>,
}

pub fn render_graph(data: &GraphData, frame: &mut ratatui::Frame, area: Rect) {
    let lines = build_graph_lines(data);
    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(" Graph "))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn build_graph_lines(data: &GraphData) -> Vec<Line<'static>> {
    if data.entities.is_empty() {
        return vec![Line::from("  No entities to display.")];
    }

    let entity_map: HashMap<&str, &Entity> = data
        .entities
        .iter()
        .map(|e| (e.id().as_str(), e))
        .collect();

    // Build adjacency: source blocks target → target depends on source
    // We'll show "blocks" edges as parent → child
    let mut children: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut has_parent: HashSet<&str> = HashSet::new();

    for rel in &data.relations {
        if rel.relation_type == RelationType::Blocks || rel.relation_type == RelationType::DependsOn
        {
            let (parent, child) = match rel.relation_type {
                RelationType::Blocks => (rel.source_id.as_str(), rel.target_id.as_str()),
                RelationType::DependsOn => (rel.target_id.as_str(), rel.source_id.as_str()),
                _ => continue,
            };
            // Only include if both entities are in our set
            if entity_map.contains_key(parent) && entity_map.contains_key(child) {
                children.entry(parent).or_default().push(child);
                has_parent.insert(child);
            }
        }
    }

    // Roots: entities with no parent
    let roots: Vec<&str> = data
        .entities
        .iter()
        .filter(|e| !has_parent.contains(e.id().as_str()))
        .map(|e| e.id().as_str())
        .collect();

    let mut lines = Vec::new();
    let mut visited: HashSet<&str> = HashSet::new();

    for root in &roots {
        render_tree(
            root,
            &entity_map,
            &children,
            &mut visited,
            &mut lines,
            "",
            true,
        );
    }

    // Show orphans (entities not connected to any blocks/depends_on)
    let shown: HashSet<&str> = visited;
    let orphans: Vec<&Entity> = data
        .entities
        .iter()
        .filter(|e| !shown.contains(e.id().as_str()))
        .collect();

    if !orphans.is_empty() && !shown.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  ── Unconnected ──",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::DIM),
        )));
    }
    for entity in orphans {
        lines.push(format_entity_line("  ", entity));
    }

    lines
}

fn render_tree<'a>(
    id: &'a str,
    entities: &HashMap<&str, &Entity>,
    children: &HashMap<&str, Vec<&'a str>>,
    visited: &mut HashSet<&'a str>,
    lines: &mut Vec<Line<'static>>,
    prefix: &str,
    is_last: bool,
) {
    if !visited.insert(id) {
        // Cycle guard
        if let Some(entity) = entities.get(id) {
            let connector = if is_last { "└─ " } else { "├─ " };
            let label = format!(
                "{prefix}{connector}↻ {} [{}]",
                entity.name(),
                entity.slug()
            );
            lines.push(Line::from(Span::styled(label, Style::default().fg(Color::Red))));
        }
        return;
    }

    let Some(entity) = entities.get(id) else {
        return;
    };

    let connector = if is_last { "└─ " } else { "├─ " };
    let indent = format!("{prefix}{connector}");
    lines.push(format_entity_line(&indent, entity));

    if let Some(kids) = children.get(id) {
        let child_prefix = if is_last {
            format!("{prefix}   ")
        } else {
            format!("{prefix}│  ")
        };
        for (i, child) in kids.iter().enumerate() {
            let child_is_last = i == kids.len() - 1;
            render_tree(child, entities, children, visited, lines, &child_prefix, child_is_last);
        }
    }
}

fn format_entity_line(prefix: &str, entity: &Entity) -> Line<'static> {
    let status_style = match entity.status() {
        EntityStatus::Open => Style::default().fg(Color::Green),
        EntityStatus::InProgress => Style::default().fg(Color::Yellow),
        EntityStatus::Blocked => Style::default().fg(Color::Red),
        EntityStatus::Closed => Style::default().fg(Color::DarkGray),
    };

    let status_icon = match entity.status() {
        EntityStatus::Open => "○",
        EntityStatus::InProgress => "◐",
        EntityStatus::Blocked => "✗",
        EntityStatus::Closed => "●",
    };

    Line::from(vec![
        Span::raw(prefix.to_string()),
        Span::styled(
            format!("{status_icon} "),
            status_style,
        ),
        Span::styled(
            entity.slug().to_string(),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::raw(entity.name().to_string()),
        Span::styled(
            format!(" [P{}]", entity.priority()),
            Style::default().fg(Color::Cyan),
        ),
    ])
}

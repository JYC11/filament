use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use std::collections::HashMap;

use filament_core::models::{Entity, EntityType, Event, LessonFields, Relation};

pub struct DetailData {
    pub entity: Entity,
    pub relations: Vec<Relation>,
    pub events: Vec<Event>,
    pub blocker_depth: usize,
    /// Maps entity ID -> "slug name" for human-readable display of related entities.
    pub name_map: HashMap<String, String>,
}

pub fn render_detail(data: &DetailData, scroll: u16, frame: &mut ratatui::Frame, area: Rect) {
    let lines = build_detail_lines(data);

    let title = format!(" [{}] {} ", data.entity.slug(), data.entity.name());

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));

    frame.render_widget(paragraph, area);
}

fn build_detail_lines(data: &DetailData) -> Vec<Line<'static>> {
    let entity = &data.entity;
    let c = entity.common();
    let mut lines = Vec::new();

    // Header
    lines.push(Line::from(vec![
        Span::styled(
            format!("{} ", entity.entity_type().as_str()),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("{} ", c.status), status_style(c.status)),
        Span::raw(format!("P{}", c.priority)),
    ]));
    lines.push(Line::from(""));

    // Summary
    if !c.summary.is_empty() {
        lines.push(section_header("Summary"));
        lines.push(Line::from(format!("  {}", c.summary)));
        lines.push(Line::from(""));
    }

    // Lesson fields (for Lesson entities)
    if entity.entity_type() == EntityType::Lesson {
        if let Some(fields) = LessonFields::from_entity(entity) {
            lines.push(section_header("Problem"));
            lines.push(Line::from(format!("  {}", fields.problem)));
            lines.push(Line::from(""));
            lines.push(section_header("Solution"));
            lines.push(Line::from(format!("  {}", fields.solution)));
            lines.push(Line::from(""));
            lines.push(section_header("Learned"));
            lines.push(Line::from(format!("  {}", fields.learned)));
            if let Some(ref pat) = fields.pattern {
                lines.push(Line::from(format!("  Pattern: {pat}")));
            }
            lines.push(Line::from(""));
        }
    } else if !c.key_facts.is_null() && c.key_facts != serde_json::json!({}) {
        // Non-lesson key facts as pretty JSON
        lines.push(section_header("Key Facts"));
        if let Ok(pretty) = serde_json::to_string_pretty(&c.key_facts) {
            for line in pretty.lines() {
                lines.push(Line::from(format!("  {line}")));
            }
        }
        lines.push(Line::from(""));
    }

    // Blocker depth (tasks only)
    if entity.entity_type() == EntityType::Task && data.blocker_depth > 0 {
        lines.push(section_header("Blocker Depth"));
        let label = if data.blocker_depth == 1 {
            "layer"
        } else {
            "layers"
        };
        lines.push(Line::from(format!(
            "  {} {label} of unclosed prerequisites",
            data.blocker_depth
        )));
        lines.push(Line::from(""));
    }

    append_relations(&mut lines, entity, &data.relations, &data.name_map);
    append_events(&mut lines, &data.events);

    lines
}

fn append_relations(
    lines: &mut Vec<Line<'static>>,
    entity: &Entity,
    relations: &[Relation],
    name_map: &HashMap<String, String>,
) {
    if relations.is_empty() {
        return;
    }
    lines.push(section_header("Relations"));
    for rel in relations {
        let (direction, other_id) = if rel.source_id.as_str() == entity.id().as_str() {
            ("->", &rel.target_id)
        } else {
            ("<-", &rel.source_id)
        };
        let other_label = resolve_name(name_map, other_id.as_str());
        let rel_type = rel.relation_type.as_str();
        let summary_suffix = if rel.summary.is_empty() {
            String::new()
        } else {
            format!(" ({})", rel.summary)
        };
        lines.push(Line::from(format!(
            "  {direction} {rel_type} {other_label}{summary_suffix}"
        )));
    }
    lines.push(Line::from(""));
}

fn append_events(lines: &mut Vec<Line<'static>>, events: &[Event]) {
    if events.is_empty() {
        return;
    }
    lines.push(section_header("Events"));
    for event in events.iter().rev().take(20) {
        let ts = event.created_at.format("%m-%d %H:%M:%S");
        let diff_summary = event
            .diff
            .as_deref()
            .map(summarize_diff)
            .unwrap_or_default();
        lines.push(Line::from(vec![
            Span::styled(format!("  {ts} "), Style::default().fg(Color::DarkGray)),
            Span::raw(format!("{}", event.event_type)),
            Span::styled(
                if diff_summary.is_empty() {
                    String::new()
                } else {
                    format!(" {diff_summary}")
                },
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }
}

fn section_header(title: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!("  {title}"),
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    ))
}

fn status_style(status: filament_core::models::EntityStatus) -> Style {
    match status {
        filament_core::models::EntityStatus::Open => Style::default().fg(Color::Green),
        filament_core::models::EntityStatus::InProgress => Style::default().fg(Color::Yellow),
        filament_core::models::EntityStatus::Blocked => Style::default().fg(Color::Red),
        filament_core::models::EntityStatus::Closed => Style::default().fg(Color::DarkGray),
    }
}

fn resolve_name(name_map: &HashMap<String, String>, id: &str) -> String {
    name_map.get(id).cloned().unwrap_or_else(|| id.to_string())
}

fn summarize_diff(diff_json: &str) -> String {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(diff_json) else {
        return String::new();
    };
    let Some(obj) = v.as_object() else {
        return String::new();
    };
    let fields: Vec<&str> = obj.keys().map(String::as_str).collect();
    if fields.is_empty() {
        String::new()
    } else {
        format!("[{}]", fields.join(", "))
    }
}

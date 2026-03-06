use ratatui::layout::Constraint;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Cell, Row, Table, TableState};

use filament_core::models::{EntityStatus, EntityType};

use crate::app::{EntityRow, FilterState, SortState};

pub fn render_entity_table<'a>(
    entities: &'a [EntityRow],
    filter: &FilterState,
    sort: &SortState,
    page: usize,
    total_pages: usize,
) -> Table<'a> {
    let label = filter.label();
    let sort_label = sort.label();
    let page_info = if total_pages > 1 {
        format!(" | {}/{}", page + 1, total_pages)
    } else {
        String::new()
    };
    let title = format!(" Entities [{label} | {sort_label}{page_info}] ");

    let is_task_only = filter.ready_only || filter.is_single_type(EntityType::Task);
    let is_lesson_only = !filter.ready_only && filter.is_single_type(EntityType::Lesson);

    if is_task_only {
        render_task_columns(entities, &title)
    } else if is_lesson_only {
        render_lesson_columns(entities, &title)
    } else {
        render_generic_columns(entities, &title)
    }
}

fn render_task_columns<'a>(entities: &'a [EntityRow], title: &str) -> Table<'a> {
    let header = Row::new(vec![
        Cell::from("Slug"),
        Cell::from("Name"),
        Cell::from("Status"),
        Cell::from("Pri"),
        Cell::from("Blocked"),
        Cell::from("Impact"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD))
    .bottom_margin(1);

    let rows: Vec<Row> = entities
        .iter()
        .map(|row| {
            let status = *row.entity.status();
            Row::new(vec![
                Cell::from(row.entity.slug().as_str().to_string()),
                Cell::from(row.entity.name().as_str().to_string()),
                Cell::from(Span::styled(status.as_str(), status_color(status))),
                Cell::from(row.entity.priority().value().to_string()),
                Cell::from(row.blocked_by_count.to_string()),
                Cell::from(row.impact.to_string()),
            ])
        })
        .collect();

    Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Min(20),
            Constraint::Length(12),
            Constraint::Length(5),
            Constraint::Length(8),
            Constraint::Length(8),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(title.to_string()),
    )
    .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED))
}

fn render_lesson_columns<'a>(entities: &'a [EntityRow], title: &str) -> Table<'a> {
    let header = Row::new(vec![
        Cell::from("Slug"),
        Cell::from("Name"),
        Cell::from("Pattern"),
        Cell::from("Status"),
        Cell::from("Pri"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD))
    .bottom_margin(1);

    let rows: Vec<Row> = entities
        .iter()
        .map(|row| {
            let status = *row.entity.status();
            let pattern = row
                .entity
                .common()
                .key_facts
                .get("pattern")
                .and_then(|v| v.as_str())
                .unwrap_or("-")
                .to_string();

            Row::new(vec![
                Cell::from(row.entity.slug().as_str().to_string()),
                Cell::from(row.entity.name().as_str().to_string()),
                Cell::from(pattern),
                Cell::from(Span::styled(status.as_str(), status_color(status))),
                Cell::from(row.entity.priority().value().to_string()),
            ])
        })
        .collect();

    Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Min(20),
            Constraint::Length(16),
            Constraint::Length(12),
            Constraint::Length(5),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(title.to_string()),
    )
    .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED))
}

fn render_generic_columns<'a>(entities: &'a [EntityRow], title: &str) -> Table<'a> {
    let header = Row::new(vec![
        Cell::from("Slug"),
        Cell::from("Name"),
        Cell::from("Type"),
        Cell::from("Status"),
        Cell::from("Pri"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD))
    .bottom_margin(1);

    let rows: Vec<Row> = entities
        .iter()
        .map(|row| {
            let status = *row.entity.status();
            Row::new(vec![
                Cell::from(row.entity.slug().as_str().to_string()),
                Cell::from(row.entity.name().as_str().to_string()),
                Cell::from(row.entity.entity_type().as_str()),
                Cell::from(Span::styled(status.as_str(), status_color(status))),
                Cell::from(row.entity.priority().value().to_string()),
            ])
        })
        .collect();

    Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Min(20),
            Constraint::Length(10),
            Constraint::Length(12),
            Constraint::Length(5),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(title.to_string()),
    )
    .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED))
}

pub fn render_entity_table_stateful(
    table: Table<'_>,
    state: &mut TableState,
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
) {
    frame.render_stateful_widget(table, area, state);
}

fn status_color(status: EntityStatus) -> Style {
    match status {
        EntityStatus::Open => Style::default().fg(Color::Green),
        EntityStatus::InProgress => Style::default().fg(Color::Yellow),
        EntityStatus::Blocked => Style::default().fg(Color::Red),
        EntityStatus::Closed => Style::default().fg(Color::DarkGray),
    }
}

use ratatui::layout::Constraint;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Cell, Row, Table, TableState};

use filament_core::models::EntityStatus;

use crate::app::TaskRow;

pub fn render_task_table<'a>(tasks: &'a [TaskRow], filter_label: &str) -> Table<'a> {
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

    let rows: Vec<Row> = tasks
        .iter()
        .map(|row| {
            let status = row.entity.status();
            let status_style = status_color(status);

            Row::new(vec![
                Cell::from(row.entity.slug().as_str().to_string()),
                Cell::from(row.entity.name().as_str().to_string()),
                Cell::from(Span::styled(status.as_str(), status_style)),
                Cell::from(row.entity.priority().value().to_string()),
                Cell::from(row.blocked_by_count.to_string()),
                Cell::from(row.impact.to_string()),
            ])
        })
        .collect();

    let title = format!(" Tasks [filter: {filter_label}] ");

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
    .block(Block::default().borders(Borders::ALL).title(title))
    .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED))
}

pub fn render_task_table_stateful(
    tasks: &[TaskRow],
    filter_label: &str,
    state: &mut TableState,
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
) {
    let table = render_task_table(tasks, filter_label);
    frame.render_stateful_widget(table, area, state);
}

fn status_color(status: &EntityStatus) -> Style {
    match status {
        EntityStatus::Open => Style::default().fg(Color::Green),
        EntityStatus::InProgress => Style::default().fg(Color::Yellow),
        EntityStatus::Blocked => Style::default().fg(Color::Red),
        EntityStatus::Closed => Style::default().fg(Color::DarkGray),
    }
}

use chrono::Utc;
use ratatui::layout::Constraint;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Cell, Row, Table, TableState};

use filament_core::models::Reservation;

pub fn render_reservation_table(reservations: &[Reservation]) -> Table<'_> {
    let header = Row::new(vec![
        Cell::from("Agent"),
        Cell::from("Glob"),
        Cell::from("Exclusive"),
        Cell::from("Time Left"),
        Cell::from("Created"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD))
    .bottom_margin(1);

    let now = Utc::now();

    let rows: Vec<Row> = reservations
        .iter()
        .map(|res| {
            let remaining = (res.expires_at - now).num_seconds();
            let expired = remaining <= 0;
            let warning = !expired && remaining < 300; // < 5 min

            let time_left_str = if expired {
                "EXPIRED".to_string()
            } else {
                format_remaining(remaining)
            };

            let time_style = if expired {
                Style::default().fg(Color::Red).add_modifier(Modifier::DIM)
            } else if warning {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };

            let row_style = if expired {
                Style::default().add_modifier(Modifier::DIM)
            } else {
                Style::default()
            };

            let exclusive_str = if res.mode.is_exclusive() { "yes" } else { "no" };
            let created = res.created_at.format("%H:%M:%S").to_string();

            Row::new(vec![
                Cell::from(res.agent_name.to_string()),
                Cell::from(res.file_glob.to_string()),
                Cell::from(exclusive_str),
                Cell::from(Span::styled(time_left_str, time_style)),
                Cell::from(created),
            ])
            .style(row_style)
        })
        .collect();

    Table::new(
        rows,
        [
            Constraint::Length(16),
            Constraint::Min(20),
            Constraint::Length(10),
            Constraint::Length(12),
            Constraint::Length(10),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Reservations "),
    )
    .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED))
}

pub fn render_reservation_table_stateful(
    reservations: &[Reservation],
    state: &mut TableState,
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
) {
    let table = render_reservation_table(reservations);
    frame.render_stateful_widget(table, area, state);
}

fn format_remaining(secs: i64) -> String {
    super::format_seconds(secs)
}

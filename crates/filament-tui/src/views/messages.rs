use ratatui::layout::Constraint;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Cell, Row, Table, TableState};

use filament_core::dto::{Escalation, EscalationKind};

pub fn render_message_table(messages: &[Escalation]) -> Table<'_> {
    let header = Row::new(vec![
        Cell::from("Kind"),
        Cell::from("Agent"),
        Cell::from("Body"),
        Cell::from("Task"),
        Cell::from("Time"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD))
    .bottom_margin(1);

    let rows: Vec<Row> = messages
        .iter()
        .map(|esc| {
            let kind_style = kind_color(&esc.kind);
            let task_id = esc
                .task_id
                .as_deref()
                .map_or_else(|| "-".to_string(), |id| truncate(id, 12));
            let time = esc.created_at.format("%H:%M:%S").to_string();

            Row::new(vec![
                Cell::from(Span::styled(esc.kind.to_string(), kind_style)),
                Cell::from(truncate(&esc.agent_name, 14)),
                Cell::from(truncate(&esc.body, 40)),
                Cell::from(task_id),
                Cell::from(time),
            ])
        })
        .collect();

    Table::new(
        rows,
        [
            Constraint::Length(12),
            Constraint::Length(16),
            Constraint::Min(20),
            Constraint::Length(14),
            Constraint::Length(10),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(" Messages "))
    .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED))
}

pub fn render_message_table_stateful(
    messages: &[Escalation],
    state: &mut TableState,
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
) {
    let table = render_message_table(messages);
    frame.render_stateful_widget(table, area, state);
}

fn kind_color(kind: &EscalationKind) -> Style {
    match kind {
        EscalationKind::Blocker => Style::default().fg(Color::Red),
        EscalationKind::Question => Style::default().fg(Color::Yellow),
        EscalationKind::NeedsInput => Style::default().fg(Color::Magenta),
    }
}

fn truncate(s: &str, max: usize) -> String {
    filament_core::util::truncate_with_ellipsis(s, max)
}

use ratatui::layout::Constraint;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Row, Table, TableState};

pub fn render_config_table(
    config_rows: &[(String, String, String)],
    state: &mut TableState,
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
) {
    let header = Row::new(vec![
        Cell::from("Key"),
        Cell::from("Value"),
        Cell::from("Source"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD))
    .bottom_margin(1);

    let rows: Vec<Row> = config_rows
        .iter()
        .map(|(key, value, source)| {
            Row::new(vec![
                Cell::from(key.as_str()),
                Cell::from(value.as_str()),
                Cell::from(source.as_str()),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(25),
            Constraint::Min(20),
            Constraint::Length(10),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(" Config "))
    .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    frame.render_stateful_widget(table, area, state);
}

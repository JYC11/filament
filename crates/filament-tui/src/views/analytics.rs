use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

use filament_core::types::EntityId;

#[derive(Default)]
pub struct AnalyticsData {
    pub pagerank: Vec<(EntityId, String, f64)>,
    pub degree: Vec<(EntityId, String, usize, usize, usize)>,
    pub calculated: bool,
}

pub fn render_analytics(data: &AnalyticsData, frame: &mut ratatui::Frame, area: Rect) {
    if !data.calculated {
        let hint = Paragraph::new("Press Enter to calculate analytics")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Analytics "),
            );
        frame.render_widget(hint, area);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    render_pagerank(&data.pagerank, frame, chunks[0]);
    render_degree(&data.degree, frame, chunks[1]);
}

fn render_pagerank(rows: &[(EntityId, String, f64)], frame: &mut ratatui::Frame, area: Rect) {
    let header = Row::new(vec![Cell::from("Name"), Cell::from("Score")])
        .style(Style::default().add_modifier(Modifier::BOLD))
        .bottom_margin(1);

    let table_rows: Vec<Row> = rows
        .iter()
        .map(|(_, name, score)| {
            Row::new(vec![
                Cell::from(name.as_str()),
                Cell::from(format!("{score:.6}")),
            ])
        })
        .collect();

    let table = Table::new(table_rows, [Constraint::Min(20), Constraint::Length(12)])
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" PageRank (damping=0.85) "),
        );

    frame.render_widget(table, area);
}

fn render_degree(
    rows: &[(EntityId, String, usize, usize, usize)],
    frame: &mut ratatui::Frame,
    area: Rect,
) {
    let header = Row::new(vec![
        Cell::from("Name"),
        Cell::from("In"),
        Cell::from("Out"),
        Cell::from("Total"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD))
    .bottom_margin(1);

    let table_rows: Vec<Row> = rows
        .iter()
        .map(|(_, name, in_deg, out_deg, total)| {
            Row::new(vec![
                Cell::from(name.as_str()),
                Cell::from(in_deg.to_string()),
                Cell::from(out_deg.to_string()),
                Cell::from(total.to_string()),
            ])
        })
        .collect();

    let table = Table::new(
        table_rows,
        [
            Constraint::Min(20),
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Length(6),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Degree Centrality "),
    );

    frame.render_widget(table, area);
}

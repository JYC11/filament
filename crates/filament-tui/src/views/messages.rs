use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState, Wrap};

use filament_core::models::{Message, MessageStatus, MessageType};

// ---------------------------------------------------------------------------
// Message table (list view)
// ---------------------------------------------------------------------------

pub fn render_message_table(
    messages: &[Message],
    filter_label: &str,
    sort_label: &str,
    has_prev: bool,
    has_next: bool,
) -> Table<'static> {
    let header = Row::new(vec![
        Cell::from("Type"),
        Cell::from("From"),
        Cell::from("To"),
        Cell::from("Body"),
        Cell::from("Status"),
        Cell::from("Time"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD))
    .bottom_margin(1);

    let rows: Vec<Row> = messages
        .iter()
        .map(|msg| {
            let type_style = msg_type_color(&msg.msg_type);
            let status_label = match msg.status {
                MessageStatus::Unread => Span::styled("unread", Style::default().fg(Color::Yellow)),
                MessageStatus::Read => Span::styled("read", Style::default().fg(Color::DarkGray)),
            };
            let time = msg.created_at.format("%H:%M:%S").to_string();

            Row::new(vec![
                Cell::from(Span::styled(msg.msg_type.as_str().to_string(), type_style)),
                Cell::from(truncate(msg.from_agent.as_str(), 14)),
                Cell::from(truncate(msg.to_agent.as_str(), 14)),
                Cell::from(truncate(msg.body.as_str(), 40)),
                Cell::from(status_label),
                Cell::from(time),
            ])
        })
        .collect();

    Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Length(16),
            Constraint::Length(16),
            Constraint::Min(20),
            Constraint::Length(8),
            Constraint::Length(10),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(format!(
        " Messages [{filter_label}] sort:{sort_label}{} ",
        page_indicator(has_prev, has_next),
    )))
    .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED))
}

pub struct MessageTableParams<'a> {
    pub messages: &'a [Message],
    pub filter_label: &'a str,
    pub sort_label: &'a str,
    pub has_prev: bool,
    pub has_next: bool,
}

pub fn render_message_table_stateful(
    params: &MessageTableParams<'_>,
    state: &mut TableState,
    frame: &mut ratatui::Frame,
    area: Rect,
) {
    let table = render_message_table(
        params.messages,
        params.filter_label,
        params.sort_label,
        params.has_prev,
        params.has_next,
    );
    frame.render_stateful_widget(table, area, state);
}

// ---------------------------------------------------------------------------
// Message detail pane
// ---------------------------------------------------------------------------

pub struct MessageDetailData {
    pub message: Message,
    pub from_name: String,
    pub to_name: String,
    pub task_name: Option<String>,
    pub reply_to: Option<Message>,
}

pub fn render_message_detail(
    data: &MessageDetailData,
    scroll: u16,
    frame: &mut ratatui::Frame,
    area: Rect,
) {
    let lines = build_detail_lines(data);
    let title = format!(
        " {} | {} ",
        data.message.msg_type.as_str(),
        match data.message.status {
            MessageStatus::Unread => "unread",
            MessageStatus::Read => "read",
        }
    );

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

fn build_detail_lines(data: &MessageDetailData) -> Vec<Line<'static>> {
    let msg = &data.message;
    let mut lines = Vec::new();

    // Header: type + status
    lines.push(Line::from(vec![
        Span::styled(
            format!("{} ", msg.msg_type.as_str()),
            msg_type_style(&msg.msg_type),
        ),
        Span::styled(
            match msg.status {
                MessageStatus::Unread => "UNREAD",
                MessageStatus::Read => "READ",
            }
            .to_string(),
            match msg.status {
                MessageStatus::Unread => Style::default().fg(Color::Yellow),
                MessageStatus::Read => Style::default().fg(Color::DarkGray),
            },
        ),
    ]));
    lines.push(Line::from(""));

    // Routing
    lines.push(section_header("Routing"));
    lines.push(Line::from(format!("  From: {}", data.from_name)));
    lines.push(Line::from(format!("  To:   {}", data.to_name)));
    lines.push(Line::from(""));

    // Body
    lines.push(section_header("Body"));
    for line in msg.body.as_str().lines() {
        lines.push(Line::from(format!("  {line}")));
    }
    lines.push(Line::from(""));

    // Task link
    if let Some(ref task_name) = data.task_name {
        lines.push(section_header("Task"));
        lines.push(Line::from(format!("  {task_name}")));
        lines.push(Line::from(""));
    }

    // Reply chain
    if let Some(ref parent) = data.reply_to {
        lines.push(section_header("In Reply To"));
        lines.push(Line::from(format!(
            "  [{} -> {}] {}",
            parent.from_agent,
            parent.to_agent,
            truncate(parent.body.as_str(), 60)
        )));
        lines.push(Line::from(""));
    } else if msg.in_reply_to.is_some() {
        lines.push(section_header("In Reply To"));
        lines.push(Line::from("  (parent message not available)".to_string()));
        lines.push(Line::from(""));
    }

    // Timestamps
    lines.push(section_header("Timestamps"));
    lines.push(Line::from(format!(
        "  Created: {}",
        msg.created_at.format("%Y-%m-%d %H:%M:%S UTC")
    )));
    if let Some(read_at) = msg.read_at {
        lines.push(Line::from(format!(
            "  Read:    {}",
            read_at.format("%Y-%m-%d %H:%M:%S UTC")
        )));
    }

    lines
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn section_header(title: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!("  {title}"),
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    ))
}

fn msg_type_color(t: &MessageType) -> Style {
    match t {
        MessageType::Text => Style::default().fg(Color::White),
        MessageType::Question => Style::default().fg(Color::Yellow),
        MessageType::Blocker => Style::default().fg(Color::Red),
        MessageType::Artifact => Style::default().fg(Color::Cyan),
    }
}

fn msg_type_style(t: &MessageType) -> Style {
    msg_type_color(t).add_modifier(Modifier::BOLD)
}

const fn page_indicator(has_prev: bool, has_next: bool) -> &'static str {
    match (has_prev, has_next) {
        (true, true) => " ‹prev next›",
        (true, false) => " ‹prev",
        (false, true) => " next›",
        (false, false) => "",
    }
}

use super::truncate;

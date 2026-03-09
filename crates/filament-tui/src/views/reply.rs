use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::ReplyState;

pub fn render_reply(reply: &ReplyState, frame: &mut ratatui::Frame, area: Rect) {
    let type_label = reply.msg_type.as_str();
    let title = format!(
        " Reply to: {} | type: {} (t:cycle) | Enter:send Esc:cancel ",
        reply.to_agent, type_label
    );

    // Show the input buffer with cursor
    let (before, after) = reply.buffer.split_at(reply.cursor);
    let cursor_char = after.chars().next().unwrap_or(' ');
    let rest = if after.is_empty() {
        String::new()
    } else {
        after[cursor_char.len_utf8()..].to_string()
    };

    let line = Line::from(vec![
        Span::raw(format!("  {before}")),
        Span::styled(
            cursor_char.to_string(),
            Style::default().bg(Color::White).fg(Color::Black),
        ),
        Span::raw(rest),
    ]);

    let paragraph = Paragraph::new(line).block(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
    );

    frame.render_widget(paragraph, area);
}

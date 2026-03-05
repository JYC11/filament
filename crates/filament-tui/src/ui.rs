use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Tabs};
use ratatui::Frame;

use crate::app::{App, Tab};
use crate::views::{agents, messages, reservations, tasks};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // tab bar
            Constraint::Min(0),    // content
            Constraint::Length(1), // status bar
        ])
        .split(frame.area());

    draw_tab_bar(frame, app, chunks[0]);
    draw_content(frame, app, chunks[1]);
    draw_status_bar(frame, app, chunks[2]);
}

fn draw_tab_bar(frame: &mut Frame, app: &App, area: Rect) {
    let titles: Vec<Line> = Tab::ALL.iter().map(|t| Line::from(t.label())).collect();

    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title(" Filament "))
        .select(app.active_tab.index())
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_widget(tabs, area);
}

fn draw_content(frame: &mut Frame, app: &mut App, area: Rect) {
    match app.active_tab {
        Tab::Tasks => {
            let filter_label = app.task_filter.label().to_string();
            let mut state = app.task_table_state.clone();
            tasks::render_task_table_stateful(&app.tasks, &filter_label, &mut state, frame, area);
            app.task_table_state = state;
        }
        Tab::Agents => {
            let mut state = app.agent_table_state.clone();
            agents::render_agent_table_stateful(&app.agent_runs, &mut state, frame, area);
            app.agent_table_state = state;
        }
        Tab::Reservations => {
            let mut state = app.reservation_table_state.clone();
            reservations::render_reservation_table_stateful(
                &app.reservations,
                &mut state,
                frame,
                area,
            );
            app.reservation_table_state = state;
        }
        Tab::Messages => {
            let mut state = app.message_table_state.clone();
            messages::render_message_table_stateful(&app.messages, &mut state, frame, area);
            app.message_table_state = state;
        }
    }
}

fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let mode = if app.conn.is_daemon_mode() {
        "daemon"
    } else {
        "direct"
    };

    let refresh_time = app.last_refresh.format("%H:%M:%S").to_string();

    let status_text = app
        .status_message
        .as_ref()
        .map_or_else(String::new, Clone::clone);

    let escalation_span = if app.escalation_count > 0 {
        Span::styled(
            format!(" ! {} escalations ", app.escalation_count),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Red)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::raw("")
    };

    let line = Line::from(vec![
        Span::styled(
            format!(" [{mode}] "),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        escalation_span,
        Span::raw(format!(" refreshed {refresh_time} ")),
        Span::styled(status_text, Style::default().fg(Color::Yellow)),
        Span::raw(match app.active_tab {
            Tab::Tasks => " | q:quit Tab:switch r:refresh j/k:nav f:filter c:close",
            _ => " | q:quit Tab:switch r:refresh j/k:nav",
        }),
    ]);

    let bar = Paragraph::new(line);
    frame.render_widget(bar, area);
}

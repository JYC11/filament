use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Tabs};
use ratatui::Frame;

use crate::app::{App, Tab};
use crate::views::{agents, analytics, config, detail, entities, filter_bar, messages, reservations};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let has_filter_bar = app.active_tab == Tab::Entities && app.filter.active_bar.is_some();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(if has_filter_bar {
            vec![
                Constraint::Length(3), // tab bar
                Constraint::Length(1), // filter bar
                Constraint::Min(0),    // content
                Constraint::Length(1), // status bar
            ]
        } else {
            vec![
                Constraint::Length(3), // tab bar
                Constraint::Min(0),    // content
                Constraint::Length(1), // status bar
            ]
        })
        .split(frame.area());

    draw_tab_bar(frame, app, chunks[0]);

    if has_filter_bar {
        filter_bar::render_filter_bar(&app.filter, frame, chunks[1]);
        draw_content(frame, app, chunks[2]);
        draw_status_bar(frame, app, chunks[3]);
    } else {
        draw_content(frame, app, chunks[1]);
        draw_status_bar(frame, app, chunks[2]);
    }
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
    let has_detail = app.active_tab == Tab::Entities && app.detail.is_some();

    let (table_area, detail_area) = if has_detail {
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(area);
        (split[0], Some(split[1]))
    } else {
        (area, None)
    };

    match app.active_tab {
        Tab::Entities => {
            let visible = app.visible_entities();
            let mut state = app.entity_table_state.clone();
            entities::render_entity_table_stateful(
                visible,
                &app.filter,
                app.page,
                app.total_pages(),
                &mut state,
                frame,
                table_area,
            );
            app.entity_table_state = state;

            if let (Some(detail_data), Some(d_area)) = (&app.detail, detail_area) {
                detail::render_detail(detail_data, app.detail_scroll, frame, d_area);
            }
        }
        Tab::Agents => {
            let mut state = app.agent_table_state.clone();
            agents::render_agent_table_stateful(
                &app.agent_runs,
                app.agent_show_history,
                &mut state,
                frame,
                table_area,
            );
            app.agent_table_state = state;
        }
        Tab::Reservations => {
            let mut state = app.reservation_table_state.clone();
            reservations::render_reservation_table_stateful(
                &app.reservations,
                &mut state,
                frame,
                table_area,
            );
            app.reservation_table_state = state;
        }
        Tab::Messages => {
            let mut state = app.message_table_state.clone();
            messages::render_message_table_stateful(&app.messages, &mut state, frame, table_area);
            app.message_table_state = state;
        }
        Tab::Config => {
            let mut state = app.config_table_state.clone();
            config::render_config_table(&app.config_rows, &mut state, frame, table_area);
            app.config_table_state = state;
        }
        Tab::Analytics => {
            analytics::render_analytics(&app.analytics, frame, table_area);
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

    let help_text = match app.active_tab {
        Tab::Entities if app.detail.is_some() => {
            " | q:quit Esc:close j/k:scroll"
        }
        Tab::Entities => {
            " | q:quit Tab:switch r:refresh j/k:nav t:type f:status P:pri F:ready n/p:page Enter:detail"
        }
        Tab::Agents => " | q:quit Tab:switch r:refresh j/k:nav h:history",
        _ => " | q:quit Tab:switch r:refresh j/k:nav",
    };

    let entity_count = app.entities.len();
    let health_span = if app.has_cycle {
        Span::styled(
            format!(" {entity_count} entities ⚠ cycle "),
            Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::raw(format!(" {entity_count} entities "))
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
        health_span,
        Span::raw(format!("refreshed {refresh_time} ")),
        Span::styled(status_text, Style::default().fg(Color::Yellow)),
        Span::raw(help_text),
    ]);

    let bar = Paragraph::new(line);
    frame.render_widget(bar, area);
}

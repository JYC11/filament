use chrono::Utc;
use ratatui::layout::Constraint;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Cell, Row, Table, TableState};

use filament_core::models::{AgentRun, AgentStatus};

pub fn render_agent_table(runs: &[AgentRun], show_history: bool) -> Table<'_> {
    let header = Row::new(vec![
        Cell::from("Task ID"),
        Cell::from("Role"),
        Cell::from("Status"),
        Cell::from("PID"),
        Cell::from("Duration"),
        Cell::from("Started"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD))
    .bottom_margin(1);

    let rows: Vec<Row> = runs
        .iter()
        .map(|run| {
            let status_style = agent_status_color(&run.status);
            let pid_str = run.pid.map_or_else(|| "-".to_string(), |p| p.to_string());

            let duration = format_duration(run);
            let started = run.started_at.format("%H:%M:%S").to_string();

            Row::new(vec![
                Cell::from(truncate(run.task_id.as_str(), 12)),
                Cell::from(run.agent_role.as_str().to_string()),
                Cell::from(Span::styled(run.status.as_str(), status_style)),
                Cell::from(pid_str),
                Cell::from(duration),
                Cell::from(started),
            ])
        })
        .collect();

    Table::new(
        rows,
        [
            Constraint::Length(14),
            Constraint::Length(10),
            Constraint::Length(12),
            Constraint::Length(8),
            Constraint::Length(10),
            Constraint::Length(10),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(if show_history {
        " Agents (history) "
    } else {
        " Agents (running) "
    }))
    .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED))
}

pub fn render_agent_table_stateful(
    runs: &[AgentRun],
    show_history: bool,
    state: &mut TableState,
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
) {
    let table = render_agent_table(runs, show_history);
    frame.render_stateful_widget(table, area, state);
}

fn format_duration(run: &AgentRun) -> String {
    let end = run.finished_at.unwrap_or_else(Utc::now);
    let secs = (end - run.started_at).num_seconds().max(0);
    super::format_seconds(secs)
}

fn agent_status_color(status: &AgentStatus) -> Style {
    match status {
        AgentStatus::Running => Style::default().fg(Color::Green),
        AgentStatus::Completed => Style::default().fg(Color::Cyan),
        AgentStatus::Failed => Style::default().fg(Color::Red),
        AgentStatus::Blocked => Style::default().fg(Color::Yellow),
        AgentStatus::NeedsInput => Style::default().fg(Color::Magenta),
    }
}

fn truncate(s: &str, max: usize) -> String {
    filament_core::util::truncate_with_ellipsis(s, max)
}

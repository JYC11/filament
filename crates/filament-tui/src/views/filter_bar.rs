use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use filament_core::models::{EntityStatus, EntityType, Priority};

use crate::app::{FilterBar, FilterState, SortField, SortState};

pub fn render_filter_bar(
    filter: &FilterState,
    sort: &SortState,
    frame: &mut ratatui::Frame,
    area: Rect,
) {
    let Some(bar) = &filter.active_bar else {
        return;
    };

    let line = match bar {
        FilterBar::Type => type_bar_line(filter),
        FilterBar::Status => status_bar_line(filter),
        FilterBar::Priority => priority_bar_line(filter),
        FilterBar::Sort => sort_bar_line(*sort),
    };

    let paragraph = Paragraph::new(line);
    frame.render_widget(paragraph, area);
}

fn type_bar_line(filter: &FilterState) -> Line<'static> {
    let types = [
        (EntityType::Task, "1:Task"),
        (EntityType::Module, "2:Module"),
        (EntityType::Service, "3:Service"),
        (EntityType::Agent, "4:Agent"),
        (EntityType::Plan, "5:Plan"),
        (EntityType::Doc, "6:Doc"),
        (EntityType::Lesson, "7:Lesson"),
    ];

    let mut spans = vec![Span::styled(
        " Type: ",
        Style::default().add_modifier(Modifier::BOLD),
    )];

    for (t, label) in &types {
        let selected = filter.types.contains(t);
        let style = chip_style(selected);
        spans.push(Span::styled(format!(" {label} "), style));
        spans.push(Span::raw(" "));
    }

    spans.push(Span::styled(
        " 0:Clear ",
        Style::default().fg(Color::DarkGray),
    ));
    spans.push(Span::styled(
        " Esc:Close ",
        Style::default().fg(Color::DarkGray),
    ));

    Line::from(spans)
}

fn status_bar_line(filter: &FilterState) -> Line<'static> {
    let statuses = [
        (EntityStatus::Open, "1:Open"),
        (EntityStatus::InProgress, "2:InProgress"),
        (EntityStatus::Blocked, "3:Blocked"),
        (EntityStatus::Closed, "4:Closed"),
    ];

    let mut spans = vec![Span::styled(
        " Status: ",
        Style::default().add_modifier(Modifier::BOLD),
    )];

    for (s, label) in &statuses {
        let selected = filter.statuses.contains(s);
        let style = chip_style(selected);
        spans.push(Span::styled(format!(" {label} "), style));
        spans.push(Span::raw(" "));
    }

    spans.push(Span::styled(
        " 0:Clear ",
        Style::default().fg(Color::DarkGray),
    ));
    spans.push(Span::styled(
        " Esc:Close ",
        Style::default().fg(Color::DarkGray),
    ));

    Line::from(spans)
}

fn priority_bar_line(filter: &FilterState) -> Line<'static> {
    let mut spans = vec![Span::styled(
        " Priority: ",
        Style::default().add_modifier(Modifier::BOLD),
    )];

    for i in 0..=4u8 {
        let p = Priority::new(i).expect("0-4 is valid");
        let selected = filter.priorities.contains(&p);
        let style = chip_style(selected);
        spans.push(Span::styled(format!(" {}:P{i} ", i + 1), style));
        spans.push(Span::raw(" "));
    }

    spans.push(Span::styled(
        " 0:Clear  Esc:Close ",
        Style::default().fg(Color::DarkGray),
    ));

    Line::from(spans)
}

fn sort_bar_line(sort: SortState) -> Line<'static> {
    let fields = [
        (SortField::Name, "1:Name"),
        (SortField::Priority, "2:Priority"),
        (SortField::Status, "3:Status"),
        (SortField::Updated, "4:Updated"),
        (SortField::Created, "5:Created"),
        (SortField::Impact, "6:Impact"),
    ];

    let mut spans = vec![Span::styled(
        " Sort: ",
        Style::default().add_modifier(Modifier::BOLD),
    )];

    for (f, label) in &fields {
        let selected = sort.field == *f;
        let style = chip_style(selected);
        let arrow = if selected { sort.direction.arrow() } else { "" };
        spans.push(Span::styled(format!(" {label}{arrow} "), style));
        spans.push(Span::raw(" "));
    }

    spans.push(Span::styled(
        " Esc:Close ",
        Style::default().fg(Color::DarkGray),
    ));

    Line::from(spans)
}

fn chip_style(selected: bool) -> Style {
    if selected {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::app::App;
use voter::nostr::events::ElectionStatus;

pub fn render(app: &App, frame: &mut Frame, election_id: &str) {
    let election = match app.elections.get(election_id) {
        Some(e) => e,
        None => {
            let msg = Paragraph::new("Election not found").style(Style::default().fg(Color::Red));
            frame.render_widget(msg, frame.area());
            return;
        }
    };

    let chunks = Layout::vertical([
        Constraint::Length(5),
        Constraint::Min(0),
        Constraint::Length(4),
        Constraint::Length(1),
    ])
    .split(frame.area());

    // Header
    let status_color = match election.status {
        ElectionStatus::Open => Color::Green,
        ElectionStatus::InProgress => Color::Yellow,
        ElectionStatus::Finished => Color::Blue,
        ElectionStatus::Cancelled => Color::Red,
    };

    let header = Paragraph::new(vec![
        Line::from(Span::styled(
            &election.name,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::raw("Status: "),
            Span::styled(
                format!("{}", election.status),
                Style::default().fg(status_color),
            ),
            Span::raw(format!(
                "  |  Rules: {}  |  {} candidates",
                election.rules_id,
                election.candidates.len()
            )),
        ]),
        Line::from(format!(
            "Start: {}  |  End: {}",
            voter::nostr::events::format_unix_utc(election.start_time),
            voter::nostr::events::format_unix_utc(election.end_time)
        )),
    ])
    .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(header, chunks[0]);

    // Candidates
    let items: Vec<ListItem> = election
        .candidates
        .iter()
        .map(|c| ListItem::new(format!("  {}. {}", c.id, c.name)))
        .collect();

    let candidates =
        List::new(items).block(Block::default().borders(Borders::ALL).title(" Candidates "));
    frame.render_widget(candidates, chunks[1]);

    // Actions
    let is_registered = app.persistent_state.is_registered(election_id);
    let has_token = app.persistent_state.get_active_token(election_id).is_some();
    let has_voted = app.persistent_state.has_voted(election_id);
    let has_results = app.results.contains_key(election_id);

    let mut actions = vec![];

    if has_voted {
        actions.push(Line::from(Span::styled(
            " Voted ",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )));
    } else if has_token {
        actions.push(Line::from(vec![
            Span::styled("[v] ", Style::default().fg(Color::Yellow)),
            Span::raw("Cast your vote"),
        ]));
    } else if is_registered && matches!(election.status, ElectionStatus::InProgress) {
        actions.push(Line::from(vec![
            Span::styled("[t] ", Style::default().fg(Color::Yellow)),
            Span::raw("Request voting token"),
        ]));
    } else if !is_registered && matches!(election.status, ElectionStatus::Open) {
        actions.push(Line::from(vec![
            Span::styled("[Enter] ", Style::default().fg(Color::Yellow)),
            Span::raw("Enter registration token to register"),
        ]));
    }

    if has_results {
        actions.push(Line::from(vec![
            Span::styled("[r] ", Style::default().fg(Color::Yellow)),
            Span::raw("View results"),
        ]));
    }

    if let Some(ref step) = app.loading_step.as_ref().filter(|_| app.is_loading) {
        actions.push(Line::from(Span::styled(
            format!("  {step}"),
            Style::default().fg(Color::Yellow),
        )));
    }

    if let Some(ref err) = app.error_message {
        actions.push(Line::from(Span::styled(
            err.as_str(),
            Style::default().fg(Color::Red),
        )));
    }

    let actions_widget =
        Paragraph::new(actions).block(Block::default().borders(Borders::ALL).title(" Actions "));
    frame.render_widget(actions_widget, chunks[2]);

    // Key hints
    let hints = Paragraph::new(Line::from(vec![
        Span::styled("Esc", Style::default().fg(Color::Yellow)),
        Span::raw(" back  "),
        Span::styled("?", Style::default().fg(Color::Yellow)),
        Span::raw(" help"),
    ]));
    frame.render_widget(hints, chunks[3]);
}

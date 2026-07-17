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

    if app.editing_token {
        actions.push(Line::from(vec![
            Span::raw("Registration token: "),
            Span::styled(
                app.token_input.as_str(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled("█", Style::default().fg(Color::Gray)),
        ]));
        actions.push(Line::from(vec![
            Span::styled("Enter", Style::default().fg(Color::Yellow)),
            Span::raw(" submit  "),
            Span::styled("Esc", Style::default().fg(Color::Yellow)),
            Span::raw(" cancel"),
        ]));
    } else if has_voted {
        actions.push(Line::from(Span::styled(
            " Voted ",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )));
    } else if has_token && matches!(election.status, ElectionStatus::InProgress) {
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

#[cfg(test)]
mod tests {
    use super::render;
    use crate::ui::test_support::{
        registration, render_to_text, sample_election, test_app, voting_token,
    };
    use voter::nostr::events::{ElectionResults, ElectionStatus, TallyEntry};

    fn app_with_election(status: ElectionStatus) -> crate::app::App {
        let (mut app, _dir) = test_app();
        app.elections.insert(
            "e1".to_string(),
            sample_election("e1", "Board Election", status, "plurality"),
        );
        app
    }

    #[test]
    fn renders_not_found_message_for_unknown_election() {
        // Arrange
        let (app, _dir) = test_app();

        // Act
        let text = render_to_text(80, 24, |f| render(&app, f, "missing"));

        // Assert
        assert!(text.contains("Election not found"));
    }

    #[test]
    fn renders_header_candidates_and_register_action_when_open_and_unregistered() {
        // Arrange
        let app = app_with_election(ElectionStatus::Open);

        // Act
        let text = render_to_text(80, 24, |f| render(&app, f, "e1"));

        // Assert
        assert!(text.contains("Board Election"));
        assert!(text.contains("Status: Open"));
        assert!(text.contains("Rules: plurality"));
        assert!(text.contains("3 candidates"));
        assert!(text.contains("UTC"));
        assert!(text.contains("1. Alice"));
        assert!(text.contains("2. Bob"));
        assert!(text.contains("3. Carol"));
        assert!(text.contains("[Enter]"));
        assert!(text.contains("Enter registration token to register"));
    }

    #[test]
    fn offers_token_request_when_registered_and_in_progress() {
        // Arrange
        let mut app = app_with_election(ElectionStatus::InProgress);
        app.persistent_state
            .registrations
            .insert("e1".to_string(), registration());

        // Act
        let text = render_to_text(80, 24, |f| render(&app, f, "e1"));

        // Assert
        assert!(text.contains("Status: In Progress"));
        assert!(text.contains("[t]"));
        assert!(text.contains("Request voting token"));
    }

    #[test]
    fn offers_vote_and_results_actions_when_token_held_and_results_published() {
        // Arrange
        let mut app = app_with_election(ElectionStatus::InProgress);
        app.persistent_state
            .tokens
            .insert("e1".to_string(), voting_token(false));
        app.results.insert(
            "e1".to_string(),
            ElectionResults {
                election_id: "e1".to_string(),
                elected: vec![1],
                tally: vec![TallyEntry {
                    candidate_id: 1,
                    votes: 1.0,
                }],
            },
        );

        // Act
        let text = render_to_text(80, 24, |f| render(&app, f, "e1"));

        // Assert
        assert!(text.contains("[v]"));
        assert!(text.contains("Cast your vote"));
        assert!(text.contains("[r]"));
        assert!(text.contains("View results"));
    }

    #[test]
    fn shows_voted_badge_after_token_is_consumed() {
        // Arrange
        let mut app = app_with_election(ElectionStatus::InProgress);
        app.persistent_state
            .tokens
            .insert("e1".to_string(), voting_token(true));

        // Act
        let text = render_to_text(80, 24, |f| render(&app, f, "e1"));

        // Assert
        assert!(text.contains("Voted"));
        assert!(!text.contains("Cast your vote"));
    }

    #[test]
    fn renders_token_input_line_while_editing_token() {
        // Arrange
        let mut app = app_with_election(ElectionStatus::Open);
        app.editing_token = true;
        app.token_input = "abc123".to_string();

        // Act
        let text = render_to_text(80, 24, |f| render(&app, f, "e1"));

        // Assert
        assert!(text.contains("Registration token: abc123█"));
        assert!(text.contains("submit"));
        assert!(text.contains("cancel"));
    }

    #[test]
    fn shows_loading_step_while_a_request_is_in_flight() {
        // Arrange: Finished status leaves the actions box free for the step
        let mut app = app_with_election(ElectionStatus::Finished);
        app.is_loading = true;
        app.loading_step = Some("Registering with EC…".to_string());

        // Act
        let text = render_to_text(80, 24, |f| render(&app, f, "e1"));

        // Assert
        assert!(text.contains("Registering with EC…"));
    }

    #[test]
    fn hides_loading_step_when_not_loading() {
        // Arrange: stale step text without the loading flag must not render
        let mut app = app_with_election(ElectionStatus::Finished);
        app.is_loading = false;
        app.loading_step = Some("Registering with EC…".to_string());

        // Act
        let text = render_to_text(80, 24, |f| render(&app, f, "e1"));

        // Assert
        assert!(!text.contains("Registering with EC…"));
    }

    #[test]
    fn renders_error_message_in_actions_box() {
        // Arrange
        let mut app = app_with_election(ElectionStatus::Finished);
        app.error_message = Some("EC did not respond in time".to_string());

        // Act
        let text = render_to_text(80, 24, |f| render(&app, f, "e1"));

        // Assert
        assert!(text.contains("EC did not respond in time"));
    }
}

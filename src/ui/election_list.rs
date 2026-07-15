use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::app::App;
use voter::nostr::events::ElectionStatus;

pub fn render(app: &App, frame: &mut Frame) {
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(frame.area());

    // Title
    let title = Paragraph::new(Line::from(vec![Span::styled(
        " Elections ",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )]))
    .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(title, chunks[0]);

    // Election list
    let election_ids = app.sorted_election_ids();

    if election_ids.is_empty() {
        let empty = Paragraph::new("No elections found. Waiting for announcements...")
            .style(Style::default().fg(Color::DarkGray))
            .centered();
        frame.render_widget(empty, chunks[1]);
    } else {
        let items: Vec<ListItem> = election_ids
            .iter()
            .enumerate()
            .map(|(i, eid)| {
                let election = &app.elections[eid];
                let status_color = match election.status {
                    ElectionStatus::Open => Color::Green,
                    ElectionStatus::InProgress => Color::Yellow,
                    ElectionStatus::Finished => Color::Blue,
                    ElectionStatus::Cancelled => Color::Red,
                };

                let voted = if app.persistent_state.has_voted(eid) {
                    Span::styled(" [voted]", Style::default().fg(Color::Green))
                } else if app.persistent_state.is_registered(eid) {
                    Span::styled(" [registered]", Style::default().fg(Color::Cyan))
                } else {
                    Span::raw("")
                };

                let style = if i == app.election_list_index {
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD | Modifier::REVERSED)
                } else {
                    Style::default()
                };

                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!(" [{}] ", election.status),
                        Style::default().fg(status_color),
                    ),
                    Span::styled(&election.name, style),
                    Span::raw(format!(
                        " ({} candidates, {})",
                        election.candidates.len(),
                        election.rules_id
                    )),
                    voted,
                ]))
            })
            .collect();

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Available Elections "),
        );
        frame.render_widget(list, chunks[1]);
    }

    // Key hints
    let hints = Paragraph::new(Line::from(vec![
        Span::styled("j/k", Style::default().fg(Color::Yellow)),
        Span::raw(" navigate  "),
        Span::styled("Enter", Style::default().fg(Color::Yellow)),
        Span::raw(" select  "),
        Span::styled("s", Style::default().fg(Color::Yellow)),
        Span::raw(" settings  "),
        Span::styled("?", Style::default().fg(Color::Yellow)),
        Span::raw(" help  "),
        Span::styled("q", Style::default().fg(Color::Yellow)),
        Span::raw(" quit"),
    ]));
    frame.render_widget(hints, chunks[2]);
}

#[cfg(test)]
mod tests {
    use super::render;
    use crate::ui::test_support::{
        buffer_text, registration, render_to_terminal, render_to_text, sample_election, test_app,
        voting_token,
    };
    use ratatui::style::Modifier;
    use voter::nostr::events::ElectionStatus;

    #[test]
    fn renders_placeholder_and_hints_when_no_elections_exist() {
        // Arrange
        let app = test_app();

        // Act
        let text = render_to_text(80, 24, |f| render(&app, f));

        // Assert
        assert!(text.contains("Elections"));
        assert!(text.contains("No elections found. Waiting for announcements..."));
        assert!(text.contains("j/k"));
        assert!(text.contains("navigate"));
        assert!(text.contains("quit"));
    }

    #[test]
    fn renders_every_status_variant_with_name_and_candidate_count() {
        // Arrange
        let mut app = test_app();
        for (id, name, status) in [
            ("e1", "Alpha", ElectionStatus::Open),
            ("e2", "Beta", ElectionStatus::InProgress),
            ("e3", "Gamma", ElectionStatus::Finished),
            ("e4", "Delta", ElectionStatus::Cancelled),
        ] {
            app.elections.insert(
                id.to_string(),
                sample_election(id, name, status, "plurality"),
            );
        }

        // Act
        let text = render_to_text(80, 24, |f| render(&app, f));

        // Assert
        assert!(text.contains("Available Elections"));
        assert!(text.contains("[Open] Alpha"));
        assert!(text.contains("[In Progress] Beta"));
        assert!(text.contains("[Finished] Gamma"));
        assert!(text.contains("[Cancelled] Delta"));
        assert!(text.contains("(3 candidates, plurality)"));
    }

    #[test]
    fn highlights_selected_row_with_reversed_style() {
        // Arrange
        let mut app = test_app();
        app.elections.insert(
            "e1".to_string(),
            sample_election("e1", "Alpha", ElectionStatus::Open, "plurality"),
        );
        app.elections.insert(
            "e2".to_string(),
            sample_election("e2", "Beta", ElectionStatus::Open, "plurality"),
        );
        app.election_list_index = 1;

        // Act
        let terminal = render_to_terminal(80, 24, |f| render(&app, f));

        // Assert: some cell carries the REVERSED modifier for the selection
        let text = buffer_text(&terminal);
        assert!(text.contains("Alpha"));
        assert!(text.contains("Beta"));
        let has_reversed = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .any(|cell| cell.modifier.contains(Modifier::REVERSED));
        assert!(has_reversed);
    }

    #[test]
    fn shows_voted_and_registered_badges() {
        // Arrange
        let mut app = test_app();
        app.elections.insert(
            "e1".to_string(),
            sample_election("e1", "Alpha", ElectionStatus::Finished, "plurality"),
        );
        app.elections.insert(
            "e2".to_string(),
            sample_election("e2", "Beta", ElectionStatus::Open, "plurality"),
        );
        app.persistent_state
            .tokens
            .insert("e1".to_string(), voting_token(true));
        app.persistent_state
            .registrations
            .insert("e2".to_string(), registration());

        // Act
        let text = render_to_text(80, 24, |f| render(&app, f));

        // Assert
        assert!(text.contains("[voted]"));
        assert!(text.contains("[registered]"));
    }
}

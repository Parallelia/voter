use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::app::App;

pub fn render(app: &App, frame: &mut Frame, election_id: &str) {
    let election = app.elections.get(election_id);
    let results = app.results.get(election_id);

    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(frame.area());

    // Title
    let name = election.map(|e| e.name.as_str()).unwrap_or("Unknown");
    let title = Paragraph::new(Line::from(Span::styled(
        format!("Results: {name}"),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )))
    .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(title, chunks[0]);

    // Results
    match results {
        Some(res) => {
            let mut entries: Vec<_> = res.tally.iter().collect();
            entries.sort_by(|a, b| {
                b.votes
                    .partial_cmp(&a.votes)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            let items: Vec<ListItem> = entries
                .iter()
                .map(|entry| {
                    let candidate_name = election
                        .and_then(|e| {
                            e.candidates
                                .iter()
                                .find(|c| c.id == entry.candidate_id)
                                .map(|c| c.name.as_str())
                        })
                        .unwrap_or("Unknown");

                    let is_winner = res.elected.contains(&entry.candidate_id);
                    let style = if is_winner {
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };

                    let winner_badge = if is_winner { " ★" } else { "" };

                    // STV tallies can be fractional (weighted transfers);
                    // show whole numbers without a decimal point.
                    let votes_str = if entry.votes.fract() == 0.0 {
                        format!("{:.0}", entry.votes)
                    } else {
                        format!("{:.2}", entry.votes)
                    };

                    ListItem::new(Line::from(vec![
                        Span::styled(format!("  {candidate_name}{winner_badge}"), style),
                        Span::raw(format!("  —  {votes_str} votes")),
                    ]))
                })
                .collect();

            let list = List::new(items).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Final Tally "),
            );
            frame.render_widget(list, chunks[1]);
        }
        None => {
            let msg = Paragraph::new("Results not yet available.")
                .style(Style::default().fg(Color::DarkGray))
                .centered();
            frame.render_widget(msg, chunks[1]);
        }
    }

    // Key hints
    let hints = Paragraph::new(Line::from(vec![
        Span::styled("Esc", Style::default().fg(Color::Yellow)),
        Span::raw(" back"),
    ]));
    frame.render_widget(hints, chunks[2]);
}

#[cfg(test)]
mod tests {
    use super::render;
    use crate::ui::test_support::{render_to_text, sample_election, test_app};
    use voter::nostr::events::{ElectionResults, ElectionStatus, TallyEntry};

    #[test]
    fn shows_placeholder_when_results_are_missing() {
        // Arrange
        let mut app = test_app();
        app.elections.insert(
            "e1".to_string(),
            sample_election(
                "e1",
                "Board Election",
                ElectionStatus::Finished,
                "plurality",
            ),
        );

        // Act
        let text = render_to_text(80, 24, |f| render(&app, f, "e1"));

        // Assert
        assert!(text.contains("Results: Board Election"));
        assert!(text.contains("Results not yet available."));
        assert!(text.contains("Esc back"));
    }

    #[test]
    fn renders_tally_sorted_descending_with_winner_badge_and_vote_formats() {
        // Arrange
        let mut app = test_app();
        app.elections.insert(
            "e1".to_string(),
            sample_election("e1", "Board Election", ElectionStatus::Finished, "stv"),
        );
        app.results.insert(
            "e1".to_string(),
            ElectionResults {
                election_id: "e1".to_string(),
                elected: vec![2],
                tally: vec![
                    TallyEntry {
                        candidate_id: 1,
                        votes: 2.5,
                    },
                    TallyEntry {
                        candidate_id: 2,
                        votes: 3.0,
                    },
                ],
            },
        );

        // Act
        let text = render_to_text(80, 24, |f| render(&app, f, "e1"));

        // Assert: winner starred, whole votes without decimals, fractional with two
        assert!(text.contains("Final Tally"));
        assert!(text.contains("Bob ★"));
        assert!(text.contains("3 votes"));
        assert!(text.contains("2.50 votes"));
        assert!(!text.contains("Alice ★"));
        let bob_pos = text.find("Bob ★").unwrap();
        let alice_pos = text.find("Alice").unwrap();
        assert!(bob_pos < alice_pos, "highest tally must be listed first");
    }

    #[test]
    fn falls_back_to_unknown_for_missing_election_and_candidate_names() {
        // Arrange: results exist but the election announcement was never seen
        let mut app = test_app();
        app.results.insert(
            "e1".to_string(),
            ElectionResults {
                election_id: "e1".to_string(),
                elected: vec![],
                tally: vec![TallyEntry {
                    candidate_id: 99,
                    votes: 1.0,
                }],
            },
        );

        // Act
        let text = render_to_text(80, 24, |f| render(&app, f, "e1"));

        // Assert
        assert!(text.contains("Results: Unknown"));
        assert!(text.contains("Unknown  —  1 votes"));
    }
}

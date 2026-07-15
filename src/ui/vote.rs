use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::app::App;

pub fn render(app: &App, frame: &mut Frame, election_id: &str) {
    let election = match app.elections.get(election_id) {
        Some(e) => e,
        None => return,
    };

    let is_stv = election.rules_id == "stv";

    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(0),
        Constraint::Length(3),
        Constraint::Length(1),
    ])
    .split(frame.area());

    // Title
    let mode = if is_stv {
        "Rank candidates in order of preference"
    } else {
        "Select one candidate"
    };
    let title = Paragraph::new(vec![
        Line::from(Span::styled(
            format!("Vote: {}", election.name),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(format!("({mode})")),
    ])
    .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(title, chunks[0]);

    // Candidate list
    let items: Vec<ListItem> = election
        .candidates
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let is_selected = app.stv_ranking.contains(&c.id);
            let rank_display = if is_stv {
                app.stv_ranking
                    .iter()
                    .position(|&id| id == c.id)
                    .map(|pos| format!("[{}] ", pos + 1))
                    .unwrap_or_else(|| "[ ] ".to_string())
            } else if is_selected {
                "(●) ".to_string()
            } else {
                "( ) ".to_string()
            };

            let cursor = if i == app.candidate_list_index {
                "▸ "
            } else {
                "  "
            };

            let style = if i == app.candidate_list_index {
                Style::default().add_modifier(Modifier::BOLD)
            } else if is_selected {
                Style::default().fg(Color::Green)
            } else {
                Style::default()
            };

            ListItem::new(Line::from(vec![
                Span::raw(cursor),
                Span::styled(
                    rank_display,
                    Style::default().fg(if is_selected {
                        Color::Green
                    } else {
                        Color::DarkGray
                    }),
                ),
                Span::styled(format!("{}. {}", c.id, c.name), style),
            ]))
        })
        .collect();

    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(" Candidates "));
    frame.render_widget(list, chunks[1]);

    // Selection summary
    let summary = if app.stv_ranking.is_empty() {
        Line::from(Span::styled(
            "No selection yet",
            Style::default().fg(Color::DarkGray),
        ))
    } else {
        let names: Vec<String> = app
            .stv_ranking
            .iter()
            .filter_map(|id| election.candidates.iter().find(|c| c.id == *id))
            .map(|c| c.name.clone())
            .collect();
        Line::from(format!(
            "Selected: {}",
            names.join(if is_stv { " > " } else { "" }.as_ref())
        ))
    };

    let submit_hint = if !app.stv_ranking.is_empty() {
        Line::from(vec![
            Span::styled("s", Style::default().fg(Color::Yellow)),
            Span::raw(" to submit your vote"),
        ])
    } else {
        Line::default()
    };

    let summary_widget = Paragraph::new(vec![summary, submit_hint]).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Your Selection "),
    );
    frame.render_widget(summary_widget, chunks[2]);

    // Key hints
    let mut hints = vec![
        Span::styled("j/k", Style::default().fg(Color::Yellow)),
        Span::raw(" navigate  "),
        Span::styled("Enter/Space", Style::default().fg(Color::Yellow)),
        Span::raw(" select  "),
    ];
    if is_stv {
        hints.push(Span::styled("d", Style::default().fg(Color::Yellow)));
        hints.push(Span::raw(" remove  "));
    }
    hints.push(Span::styled("s", Style::default().fg(Color::Yellow)));
    hints.push(Span::raw(" submit  "));
    hints.push(Span::styled("Esc", Style::default().fg(Color::Yellow)));
    hints.push(Span::raw(" back"));

    frame.render_widget(Paragraph::new(Line::from(hints)), chunks[3]);

    // Confirmation dialog overlay
    if let Some(confirm_focused) = app.vote_confirm {
        let names: Vec<String> = app
            .stv_ranking
            .iter()
            .filter_map(|id| election.candidates.iter().find(|c| c.id == *id))
            .map(|c| c.name.clone())
            .collect();
        let mut lines = vec![
            "You are about to cast your vote:".to_string(),
            String::new(),
        ];
        if is_stv {
            for (i, name) in names.iter().enumerate() {
                lines.push(format!("  {}. {}", i + 1, name));
            }
        } else {
            lines.push(format!("  {}", names.join(", ")));
        }
        lines.push(String::new());
        lines.push("This cannot be undone.".to_string());

        crate::ui::widgets::confirm_dialog::render(frame, "Confirm Vote", &lines, confirm_focused);
    }
}

#[cfg(test)]
mod tests {
    use super::render;
    use crate::ui::test_support::{render_to_text, sample_election, test_app};
    use voter::nostr::events::ElectionStatus;

    fn app_with_rules(rules_id: &str) -> crate::app::App {
        let mut app = test_app();
        app.elections.insert(
            "e1".to_string(),
            sample_election("e1", "Board Election", ElectionStatus::InProgress, rules_id),
        );
        app
    }

    #[test]
    fn renders_nothing_for_unknown_election() {
        // Arrange
        let app = test_app();

        // Act
        let text = render_to_text(80, 24, |f| render(&app, f, "missing"));

        // Assert: early return leaves the buffer blank
        assert!(text.chars().all(|c| c == ' ' || c == '\n'));
    }

    #[test]
    fn renders_plurality_ballot_with_radio_selection() {
        // Arrange
        let mut app = app_with_rules("plurality");
        app.stv_ranking = vec![2];

        // Act
        let text = render_to_text(80, 24, |f| render(&app, f, "e1"));

        // Assert
        assert!(text.contains("Vote: Board Election"));
        assert!(text.contains("(Select one candidate)"));
        assert!(text.contains("(●) 2. Bob"));
        assert!(text.contains("( ) 3. Carol"));
        assert!(text.contains("▸ ( ) 1. Alice"));
        assert!(text.contains("Selected: Bob"));
        assert!(!text.contains("remove"));
    }

    #[test]
    fn shows_no_selection_placeholder_when_ballot_empty() {
        // Arrange
        let app = app_with_rules("plurality");

        // Act
        let text = render_to_text(80, 24, |f| render(&app, f, "e1"));

        // Assert
        assert!(text.contains("No selection yet"));
        assert!(!text.contains("to submit your vote"));
    }

    #[test]
    fn renders_stv_ballot_with_rank_numbers_and_remove_hint() {
        // Arrange
        let mut app = app_with_rules("stv");
        app.stv_ranking = vec![2, 1];

        // Act
        let text = render_to_text(80, 24, |f| render(&app, f, "e1"));

        // Assert
        assert!(text.contains("(Rank candidates in order of preference)"));
        assert!(text.contains("[1] 2. Bob"));
        assert!(text.contains("[2] 1. Alice"));
        assert!(text.contains("[ ] 3. Carol"));
        assert!(text.contains("Selected: Bob > Alice"));
        assert!(text.contains("remove"));
    }

    #[test]
    fn renders_confirm_dialog_for_stv_ranking_with_confirm_focused() {
        // Arrange
        let mut app = app_with_rules("stv");
        app.stv_ranking = vec![2, 1];
        app.vote_confirm = Some(true);

        // Act: taller terminal so all dialog lines fit
        let text = render_to_text(80, 40, |f| render(&app, f, "e1"));

        // Assert
        assert!(text.contains("Confirm Vote"));
        assert!(text.contains("You are about to cast your vote:"));
        assert!(text.contains("1. Bob"));
        assert!(text.contains("2. Alice"));
        assert!(text.contains("This cannot be undone."));
        assert!(text.contains("Confirm"));
        assert!(text.contains("Go Back"));
    }

    #[test]
    fn renders_confirm_dialog_for_plurality_choice_with_back_focused() {
        // Arrange
        let mut app = app_with_rules("plurality");
        app.stv_ranking = vec![3];
        app.vote_confirm = Some(false);

        // Act
        let text = render_to_text(80, 40, |f| render(&app, f, "e1"));

        // Assert
        assert!(text.contains("Confirm Vote"));
        assert!(text.contains("Carol"));
        assert!(text.contains("This cannot be undone."));
    }
}

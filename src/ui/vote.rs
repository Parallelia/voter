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

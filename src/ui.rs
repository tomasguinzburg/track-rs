use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::CrosstermBackend,
    style::{Color, Style},
    text::Line,
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
    Frame, Terminal,
};

use crate::{state::App, tracker::PatternStep};

pub struct TUI<W: std::io::Write> {
    pub terminal: Terminal<CrosstermBackend<W>>,
}

impl<W: std::io::Write> TUI<W> {
    pub fn new(writer: W) -> anyhow::Result<Self> {
        enable_raw_mode()?;
        let mut w = writer;
        execute!(w, EnterAlternateScreen)?;

        let backend = CrosstermBackend::new(w);
        let terminal = Terminal::new(backend)?;

        Ok(TUI { terminal })
    }

    pub fn destroy(&mut self) -> anyhow::Result<()> {
        disable_raw_mode()?;
        execute!(self.terminal.backend_mut(), LeaveAlternateScreen)?;
        self.terminal.show_cursor()?;

        Ok(())
    }

    pub fn draw(&mut self, state: &App) {
        self.terminal
            .draw(|f| {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .margin(1)
                    .constraints(
                        [
                            Constraint::Percentage(2),
                            Constraint::Percentage(90),
                            Constraint::Percentage(5),
                        ]
                        .as_ref(),
                    )
                    .split(f.area());

                draw_title_bar(f, chunks[0], state);
                draw_pattern_editor(f, chunks[1], state);
                draw_status_bar(f, chunks[2], state);
            })
            .unwrap();
    }
}

fn draw_title_bar(f: &mut Frame, area: Rect, _: &App) {
    let block = Block::default()
        .borders(Borders::BOTTOM)
        .title(Line::from("track-rs").centered());
    f.render_widget(block, area);
}

fn draw_pattern_editor(f: &mut Frame, area: Rect, app_state: &App) {
    let pattern = &app_state.pattern;
    let num_tracks = pattern.tracks.len();
    let header_cells =
        (0..num_tracks).map(|i| Cell::from(format!("Track {}", i + 1)).style(Style::default().fg(Color::Green)));
    let header = Row::new(header_cells).height(1).bottom_margin(1);

    let mut rows = vec![];
    for row_idx in 0..pattern.num_rows {
        let mut row_cells = vec![];
        for track_idx in 0..num_tracks {
            let step = pattern
                .get_step(track_idx, row_idx)
                .copied()
                .expect("Should be able to clone a step");
            let cell_str = format_step(step); //TODO: Trait "drawable" or whatever, impl on patternstep

            // Highlighting
            let mut style = Style::default();
            if row_idx == app_state.cursor_row && track_idx == app_state.cursor_track {
                style = style.bg(Color::Green).fg(Color::Black); // Highlight cursor
            }

            if let crate::state::PlaybackState::Playing = app_state.playback_state {
                if row_idx == app_state.playback_row {
                    style = style.bg(Color::DarkGray); // Highlight playback caret
                }
            }

            row_cells.push(Cell::from(cell_str).style(style));
        }
        rows.push(Row::new(row_cells));
    }

    let track_width = 10; //FIXME: fixed width is not guaranteed
    let constraints = std::iter::repeat_n(Constraint::Length(track_width), num_tracks).collect::<Vec<_>>();

    let table = Table::new(rows, &constraints)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title("Pattern Editor"));

    f.render_widget(table, area);
}

fn draw_status_bar(f: &mut Frame, area: Rect, app_state: &App) {
    let status_text = format!(
        "State: {:?} | Pos: {:02},{:02} | Playback Row: {:02} | BPM: {:.1} | Octave {:02}",
        app_state.playback_state,
        app_state.cursor_row,
        app_state.cursor_track,
        app_state.playback_row,
        app_state.bpm,
        app_state.octave
    );
    let paragraph = Paragraph::new(status_text).block(Block::default().borders(Borders::ALL).title("Status"));
    f.render_widget(paragraph, area);
}

fn format_step(step: PatternStep) -> String {
    match step.note {
        Some(note) => format!("{:?}{:1}", note.pitch, note.octave),
        None => " . ".to_string(),
    }
}

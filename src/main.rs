use audio::AudioEngine;
use crossterm::{
    event::{self, Event as CEvent, KeyCode, KeyEvent},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use state::{App, CursorOffset, PlaybackState};

use ratatui::{backend::CrosstermBackend, Terminal};
use std::{
    io::{stdout, Stdout},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};
use tracker::{Note, NotePitch};

mod audio;
mod state;
mod tracker;
mod ui;

fn startup_tui() -> anyhow::Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    Ok(terminal)
}

fn startup_stdin_channel() -> Receiver<crossterm::event::KeyEvent> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        loop {
            if event::poll(Duration::from_millis(100)).expect("Polling should be fine") {
                if let CEvent::Key(key) = event::read().expect("Reading an event as well") {
                    if tx.send(key).is_err() {
                        break;
                    }
                }
            }
            //TODO: sleep the thread for lower cpu usage?
        }
    });

    rx
}

fn handle_cmd_input(key: KeyEvent, app_state: &mut App) {
    match key.code {
        KeyCode::Char('q' | 'Q') => app_state.shutdown(),
        KeyCode::Char(' ') => app_state.toggle_playback(),
        KeyCode::Char('k') => app_state.offset_cursor(&CursorOffset::Neg(1), &CursorOffset::None),
        KeyCode::Char('j') => app_state.offset_cursor(&CursorOffset::Pos(1), &CursorOffset::None),
        KeyCode::Char('h') => app_state.offset_cursor(&CursorOffset::None, &CursorOffset::Neg(1)),
        KeyCode::Char('l') => app_state.offset_cursor(&CursorOffset::None, &CursorOffset::Pos(1)),

        // TODO: inputs for notes
        KeyCode::Char('z') => app_state.insert_note_under_cursor(Some(Note {
            pitch: NotePitch::C,
            octave: app_state.octave,
        })),
        KeyCode::Char('x') => app_state.insert_note_under_cursor(Some(Note {
            pitch: NotePitch::D,
            octave: app_state.octave,
        })),
        KeyCode::Char('c') => app_state.insert_note_under_cursor(Some(Note {
            pitch: NotePitch::E,
            octave: app_state.octave,
        })),
        KeyCode::Char('v') => app_state.insert_note_under_cursor(Some(Note {
            pitch: NotePitch::F,
            octave: app_state.octave,
        })),
        KeyCode::Char('b') => app_state.insert_note_under_cursor(Some(Note {
            pitch: NotePitch::G,
            octave: app_state.octave,
        })),
        KeyCode::Char('n') => app_state.insert_note_under_cursor(Some(Note {
            pitch: NotePitch::A,
            octave: app_state.octave,
        })),
        KeyCode::Char('m') => app_state.insert_note_under_cursor(Some(Note {
            pitch: NotePitch::B,
            octave: app_state.octave,
        })),
        KeyCode::Char(',') => app_state.insert_note_under_cursor(Some(Note {
            pitch: NotePitch::C,
            octave: app_state.octave + 1,
        })),
        KeyCode::Delete => app_state.insert_note_under_cursor(None),
        KeyCode::Char('a') => app_state.octave = app_state.octave.saturating_sub(1).clamp(0, 9),
        KeyCode::Char('s') => app_state.octave = app_state.octave.saturating_add(1).clamp(0, 9),
        // KeyCode::Char('?') => open_help_floating_pane
        _ => {}
    }
}

fn main_loop<B: ratatui::backend::Backend>(
    app_state: &mut App,
    terminal: &mut Terminal<B>,
    audio_engine: &AudioEngine,
    commands_rx: &Receiver<crossterm::event::KeyEvent>,
) {
    let mut last_frame = Instant::now();
    let frame_period = Duration::from_millis(50); //UI refresh rate

    let mut last_playback_time: Option<Instant> = None;

    loop {
        if app_state.shutdown {
            break;
        }

        //Play audio
        let seconds_per_beat = 60.0 / app_state.bpm;
        let rows_per_beat = 4.0;
        let seconds_per_row = seconds_per_beat / rows_per_beat;

        assert!((seconds_per_row > 0.0), "Too fast a BPM champ!"); //FIXME: Tempo should be
                                                                   //ridiculously high for this assert to even trigger

        let playback_interval = Duration::from_secs_f64(seconds_per_row);

        match app_state.playback_state {
            PlaybackState::Playing => {
                let now = Instant::now();
                let mut trigger_playback_audio = false;

                if let Some(last_time) = last_playback_time {
                    if now.duration_since(last_time) >= playback_interval {
                        last_playback_time = Some(now);
                        trigger_playback_audio = true;
                        app_state.advance_playback_row();
                    }
                } else {
                    last_playback_time = Some(now);
                    trigger_playback_audio = true;
                }

                if trigger_playback_audio {
                    let current_row = app_state.playback_row;
                    for track_idx in 0..app_state.pattern.tracks.len() {
                        if let Some(step) = app_state.pattern.get_step(track_idx, current_row).copied() {
                            if let Some(note) = step.note {
                                audio_engine.play_note(note); //FIXME: send async message to audio
                                                              //thread?
                            }
                        }
                    }
                }
            }
            PlaybackState::Stopped => {
                last_playback_time = None;
            }
        }

        // Handle input
        // TODO: move to its own thingy
        if let Ok(key) = commands_rx.try_recv() {
            handle_cmd_input(key, app_state);
        }

        //Draw UI
        // TODO: move to its own thingy, renderer maybe?
        if last_frame.elapsed() >= frame_period {
            ui::draw(terminal, app_state);
            last_frame = Instant::now();
        }

        //Prevent Busy-Waiting
        //Calculates smallest potential wait time until next frame or next playback tick
        let time_until_next_frame = frame_period.saturating_sub(last_frame.elapsed());
        let time_until_next_tick = match (app_state.playback_state, last_playback_time) {
            (PlaybackState::Playing, Some(last_time)) => playback_interval.saturating_sub(last_time.elapsed()),
            (PlaybackState::Playing, None) => Duration::from_secs(0),
            (_, _) => time_until_next_frame,
        };

        let suggested_sleep = std::cmp::min(
            std::cmp::min(time_until_next_tick, time_until_next_frame),
            Duration::from_millis(10),
        );

        let sleep_time = if suggested_sleep >= Duration::from_millis(5) {
            suggested_sleep
        } else {
            Duration::from_millis(1)
        };

        thread::sleep(sleep_time);
    }
}

fn dismantle_tui<B: ratatui::backend::Backend + std::io::Write>(terminal: &mut Terminal<B>) -> anyhow::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Startup:
    let audio_engine = AudioEngine::new()?;
    let mut terminal = startup_tui()?;
    let mut app_state = App::new();

    let commands_rx = startup_stdin_channel();

    //Main loop
    main_loop(&mut app_state, &mut terminal, &audio_engine, &commands_rx);

    //Winddown
    dismantle_tui(&mut terminal)?;

    Ok(())
}

use std::{error::Error, io};

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{prelude::*, widgets::*};

use super::{
    app::{App, HandleCommandStatus},
    background_task,
    input_controller::*,
};

pub async fn set_tui(app: App) -> Result<(), Box<dyn Error>> {
    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Hook panic callback
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        reset_terminal().unwrap();
        original_hook(panic);
    }));

    // Task for receiving broadcast messages from server
    tokio::task::spawn(background_task::print_message_packets(
        app.incoming_tx.subscribe(),
        app.messages.clone(),
    ));

    // create app and run it
    _ = run_app(&mut terminal, app).await;

    // restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    _ = terminal.show_cursor();
    Ok(())
}

fn reset_terminal() -> Result<(), Box<dyn Error>> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;
    Ok(())
}

async fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut app: App) -> io::Result<()> {
    app.messages
        .push_sys_msg(format!("Welcome {}!", &app.state.id));
    loop {
        terminal.draw(|f| construct_ui(f, &app))?;

        // non-blocking event reading
        if !event::poll(std::time::Duration::from_millis(100))? {
            continue;
        }

        if let Event::Key(key) = event::read()? {
            match app.input_controller.input_mode {
                InputMode::Normal => match key.code {
                    KeyCode::Char('e') => {
                        app.input_controller.input_mode = InputMode::Editing;
                    }
                    KeyCode::Char('q') => {
                        return Ok(());
                    }
                    _ => {}
                },
                InputMode::Editing if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Enter => {
                        if app.input_controller.input.starts_with('/') {
                            // handle command
                            if app.handle_command().await == HandleCommandStatus::Exit {
                                return Ok(());
                            }
                            app.input_controller.clear_input_box();
                        } else {
                            app.send_message().await;
                            app.messages
                                .push(app.state.id.clone(), app.input_controller.input.clone());
                            app.input_controller.clear_input_box();
                        }
                    }
                    KeyCode::Char(ch) => app.input_controller.enter_char(ch),
                    KeyCode::Backspace => app.input_controller.delete_char(),
                    KeyCode::Left => app.input_controller.move_cursor_left(),
                    KeyCode::Right => app.input_controller.move_cursor_right(),
                    KeyCode::Esc => app.input_controller.input_mode = InputMode::Normal,
                    _ => {}
                },
                _ => {}
            }
        }
    }
}

pub fn render_help_messages(f: &mut Frame, app: &App, chunk: Rect) {
    // Helper messages
    let (msg, style) = match app.input_controller.input_mode {
        InputMode::Normal => (
            vec![
                "Press ".into(),
                "q".bold(),
                " to exit, ".into(),
                "e".bold(),
                " to start editing.".bold(),
            ],
            Style::default().add_modifier(Modifier::RAPID_BLINK),
        ),
        InputMode::Editing => (
            vec![
                "Press ".into(),
                "Esc".bold(),
                " to stop editing, ".into(),
                "Enter".bold(),
                " to record the message".into(),
            ],
            Style::default(),
        ),
    };

    f.render_widget(
        Paragraph::new({
            let mut text = Text::from(Line::from(msg));
            text.patch_style(style);
            text
        }),
        chunk,
    );
}

pub fn construct_ui(f: &mut Frame, app: &App) {
    // Layout chunks
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(3),
        ])
        .split(f.size());

    // input messages
    render_help_messages(f, app, chunks[0]);

    let input = Paragraph::new(app.input_controller.input.as_str())
        .style(match app.input_controller.input_mode {
            InputMode::Normal => Style::default(),
            InputMode::Editing => Style::default().fg(Color::Yellow),
        })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(app.state.id.clone()),
        );
    f.render_widget(input, chunks[2]);

    // Set cursor position if current input mode is Editing
    if app.input_controller.input_mode == InputMode::Editing {
        f.set_cursor(
            chunks[2].x + app.input_controller.cursor_pos as u16 + 1,
            chunks[2].y + 1,
        );
    }

    let messages = app.messages.collect_list_item();
    let messages = List::new(messages).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!("[Channel: {}]", app.state.channel.clone())),
    );
    f.render_widget(messages, chunks[1]);
}

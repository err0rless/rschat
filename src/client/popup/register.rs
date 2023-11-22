use crossterm::event::KeyCode;
use ratatui::{prelude::*, widgets::*};

use super::*;
use crate::client::input_controller::InputController;

pub struct RegisterPopupManager {
    id_input: InputController,
    password_input: InputController,
    bio_input: InputController,
    location_input: InputController,

    // index of the currently focus field
    focus_idx: usize,
}

impl RegisterPopupManager {
    pub fn new() -> Self {
        Self {
            id_input: InputController::default(),
            password_input: InputController::default(),
            bio_input: InputController::default(),
            location_input: InputController::default(),
            focus_idx: 0usize,
        }
    }

    fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
        let center_y = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ])
            .split(r);
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ])
            .split(center_y[1])[1]
    }

    /// return reference to the currently focused input controller
    fn focused_input(&self) -> &InputController {
        match self.focus_idx {
            0 => &self.id_input,
            1 => &self.password_input,
            2 => &self.bio_input,
            3 => &self.location_input,
            // Something went wrong
            _ => &self.id_input,
        }
    }

    /// return mutable reference to the currently focused input controller
    fn focused_input_mut(&mut self) -> &mut InputController {
        match self.focus_idx {
            0 => &mut self.id_input,
            1 => &mut self.password_input,
            2 => &mut self.bio_input,
            3 => &mut self.location_input,
            // Something went wrong
            _ => &mut self.id_input,
        }
    }
}

impl PopupManager for RegisterPopupManager {
    fn ui(&self, f: &mut Frame) {
        let popup_area = RegisterPopupManager::centered_rect(50, 11, f.size());

        // clear out the background
        f.render_widget(Clear, popup_area);

        let (x, y, width) = (popup_area.x, popup_area.y, popup_area.width);

        // instruction
        f.render_widget(
            Paragraph::new({
                let mut line = Line::from(vec![
                    "Esc".bold(),
                    " to cancel |".into(),
                    " Enter".bold(),
                    " to login |".into(),
                    " Tab".bold(),
                    " to switch focus".into(),
                ]);
                line.patch_style(Style::default().add_modifier(Modifier::RAPID_BLINK));
                line
            }),
            Rect::new(x, y, width, 1),
        );

        // ID input box
        f.render_widget(
            Paragraph::new(self.id_input.buf.as_str())
                .style(Style::default().fg(if self.focus_idx == 0 {
                    Color::Yellow
                } else {
                    Color::default()
                }))
                .block(Block::default().borders(Borders::ALL).title("ID")),
            Rect::new(x, y + 1, width, 3),
        );

        // Password input box
        f.render_widget(
            Paragraph::new("*".repeat(self.password_input.buf.len()))
                .style(Style::default().fg(if self.focus_idx == 1 {
                    Color::Yellow
                } else {
                    Color::default()
                }))
                .block(Block::default().borders(Borders::ALL).title("Password")),
            Rect::new(x, y + 4, width, 3),
        );

        // Password input box
        f.render_widget(
            Paragraph::new(self.bio_input.buf.as_str())
                .style(Style::default().fg(if self.focus_idx == 2 {
                    Color::Yellow
                } else {
                    Color::default()
                }))
                .block(Block::default().borders(Borders::ALL).title("bio")),
            Rect::new(x, y + 7, width, 3),
        );

        // Password input box
        f.render_widget(
            Paragraph::new(self.location_input.buf.as_str())
                .style(Style::default().fg(if self.focus_idx == 3 {
                    Color::Yellow
                } else {
                    Color::default()
                }))
                .block(Block::default().borders(Borders::ALL).title("location")),
            Rect::new(x, y + 10, width, 3),
        );

        // cursor position depends on its focusing input field
        f.set_cursor(
            x + self.focused_input().cursor_pos as u16 + 1,
            y + 1 + self.focus_idx as u16 * 3 + 1,
        );
    }

    fn hook_key_event(&mut self, key_event: &KeyEvent) -> PostKeyCaptureAction {
        match key_event.code {
            // Switch focus
            KeyCode::Tab => {
                self.focus_idx = (self.focus_idx + 1) % 4;
                PostKeyCaptureAction::Break
            }
            // Enter key entered,
            KeyCode::Enter => {
                // construct register action request
                PostKeyCaptureAction::CloseAndRunAction(
                    app::CommandAction::Register,
                    Some(serde_json::json!({
                        "id": self.id_input.buf.clone(),
                        "password": self.password_input.buf.clone(),
                        "bio": self.bio_input.buf.clone(),
                        "location": self.location_input.buf.clone(),
                    })),
                )
            }
            KeyCode::Char(ch) => {
                self.focused_input_mut().enter_char(ch);
                PostKeyCaptureAction::Break
            }
            KeyCode::Backspace => {
                self.focused_input_mut().delete_char();
                PostKeyCaptureAction::Break
            }
            KeyCode::Left => {
                self.focused_input_mut().move_cursor_left();
                PostKeyCaptureAction::Break
            }
            KeyCode::Right => {
                self.focused_input_mut().move_cursor_right();
                PostKeyCaptureAction::Break
            }
            // Cancellation
            KeyCode::Esc => PostKeyCaptureAction::ClosePopup,
            _ => PostKeyCaptureAction::Break,
        }
    }
}

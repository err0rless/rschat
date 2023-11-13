use std::sync::{Arc, Mutex};

use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
    widgets::ListItem,
};

pub enum InputMode {
    Normal,
    Editing,
}

#[derive(Default)]
pub struct MessageChannel {
    pub messages: Arc<Mutex<Vec<(String, String)>>>,
}

impl Clone for MessageChannel {
    fn clone(&self) -> Self {
        Self {
            messages: self.messages.clone(),
        }
    }
}

impl MessageChannel {
    pub fn push(&self, id: String, msg: String) {
        self.messages.lock().unwrap().push((id, msg));
    }

    pub fn collect_list_item(&self) -> Vec<ListItem> {
        let messages: Vec<ListItem> = self
            .messages
            .lock()
            .unwrap()
            .iter()
            .map(|(id, msg)| {
                let content = match &id[..] {
                    "System" => Line::from(Span::styled(
                        format!("[System]: {}", msg),
                        Style::default().fg(Color::LightBlue),
                    )),
                    "SystemError" => Line::from(Span::styled(
                        format!("[SystemError]: {}", msg),
                        Style::default().fg(Color::LightRed),
                    )),
                    _ => Line::from(Span::raw(format!("{}: {}", id, msg))),
                };
                ListItem::new(content)
            })
            .collect();
        messages
    }
}

pub struct InputController {
    /// Current input content
    pub input: String,

    /// Messages that have been submitted
    pub messages: MessageChannel,

    /// current cursor position
    pub cursor_position: usize,

    /// current input mode (Normal, Editing)
    pub input_mode: InputMode,
}

impl Default for InputController {
    fn default() -> Self {
        Self {
            input: String::new(),
            messages: MessageChannel::default(),
            input_mode: InputMode::Normal,
            cursor_position: 0,
        }
    }
}

impl InputController {
    pub fn move_cursor_left(&mut self) {
        let cursor_moved_left = self.cursor_position.saturating_sub(1);
        self.cursor_position = self.clamp_cursor(cursor_moved_left);
    }

    pub fn move_cursor_right(&mut self) {
        let cursor_moved_right = self.cursor_position.saturating_add(1);
        self.cursor_position = self.clamp_cursor(cursor_moved_right);
    }

    pub fn enter_char(&mut self, new_char: char) {
        self.input.insert(self.cursor_position, new_char);
        self.move_cursor_right();
    }

    pub fn delete_char(&mut self) {
        let is_not_cursor_leftmost = self.cursor_position != 0;
        if is_not_cursor_leftmost {
            let current_index = self.cursor_position;
            let from_left_to_current_index = current_index - 1;

            // Getting all characters before the selected character.
            let before_char_to_delete = self.input.chars().take(from_left_to_current_index);
            // Getting all characters after selected character.
            let after_char_to_delete = self.input.chars().skip(current_index);

            // Put all characters together except the selected one.
            // By leaving the selected one out, it is forgotten and therefore deleted.
            self.input = before_char_to_delete.chain(after_char_to_delete).collect();
            self.move_cursor_left();
        }
    }

    pub fn clamp_cursor(&self, new_cursor_pos: usize) -> usize {
        new_cursor_pos.clamp(0, self.input.len())
    }

    pub fn reset_cursor(&mut self) {
        self.cursor_position = 0;
    }

    pub fn clear_input_box(&mut self) {
        self.input.clear();
        self.reset_cursor();
    }

    pub fn submit_message(&mut self, id: &str) {
        self.messages.push(id.to_owned(), self.input.clone());
        self.clear_input_box();
    }
}

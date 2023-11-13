#[derive(PartialEq)]
pub enum InputMode {
    Normal,
    Editing,
}

pub struct InputController {
    /// Current input content
    pub input: String,

    /// current position of the cursor
    pub cursor_pos: usize,

    /// current input mode (Normal, Editing)
    pub input_mode: InputMode,
}

impl Default for InputController {
    fn default() -> Self {
        Self {
            input: String::new(),
            input_mode: InputMode::Editing,
            cursor_pos: 0,
        }
    }
}

impl InputController {
    pub fn clamp_cursor(&self, new_cursor_pos: usize) -> usize {
        new_cursor_pos.clamp(0, self.input.len())
    }

    pub fn move_cursor_left(&mut self) {
        let cursor_moved_left = self.cursor_pos.saturating_sub(1);
        self.cursor_pos = self.clamp_cursor(cursor_moved_left);
    }

    pub fn move_cursor_right(&mut self) {
        let cursor_moved_right = self.cursor_pos.saturating_add(1);
        self.cursor_pos = self.clamp_cursor(cursor_moved_right);
    }

    pub fn enter_char(&mut self, ch: char) {
        self.input.insert(self.cursor_pos, ch);
        self.move_cursor_right();
    }

    pub fn delete_char(&mut self) {
        if self.cursor_pos > 0 {
            self.input.remove(self.cursor_pos - 1);
            self.move_cursor_left();
        }
    }

    pub fn reset_cursor_pos(&mut self) {
        self.cursor_pos = 0;
    }

    pub fn clear_input_box(&mut self) {
        self.input.clear();
        self.reset_cursor_pos();
    }
}

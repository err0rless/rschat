#[derive(PartialEq)]
pub enum InputMode {
    Normal,
    Editing,
}

pub struct InputController {
    /// Current input content
    pub buf: String,

    /// current position of the cursor
    pub cursor_pos: usize,

    /// current input mode (Normal, Editing)
    pub input_mode: InputMode,
}

impl Default for InputController {
    fn default() -> Self {
        Self {
            buf: String::new(),
            cursor_pos: 0,
            input_mode: InputMode::Editing,
        }
    }
}

impl InputController {
    pub fn is_editing_mode(&self) -> bool {
        self.input_mode == InputMode::Editing
    }

    /// Switch to normal mode
    pub fn normal_mode(&mut self) {
        self.input_mode = InputMode::Normal;
    }

    /// Switch to editing mod
    pub fn editing_mode(&mut self) {
        self.input_mode = InputMode::Editing;
    }

    pub fn clamp_cursor(&self, new_cursor_pos: usize) -> usize {
        new_cursor_pos.clamp(0, self.buf.len())
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
        self.buf.insert(self.cursor_pos, ch);
        self.move_cursor_right();
    }

    pub fn delete_char(&mut self) {
        self.buf.pop();
        self.move_cursor_left();
    }

    pub fn reset_cursor_pos(&mut self) {
        self.cursor_pos = 0;
    }

    pub fn clear_input_box(&mut self) {
        self.buf.clear();
        self.reset_cursor_pos();
    }
}

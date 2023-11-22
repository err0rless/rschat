pub mod login;
pub mod register;

use crossterm::event::KeyEvent;
use ratatui::prelude::*;

use crate::client::app;

pub enum PostKeyCaptureAction {
    CloseAndRunAction(app::CommandAction, Option<serde_json::Value>),
    ClosePopup,
    Break,
    Fallthrough,
}

pub trait PopupManager {
    // UI drawer
    fn ui(&self, f: &mut Frame);

    // Implement this method if your popup should capture key events
    fn hook_key_event(&mut self, _: &KeyEvent) -> PostKeyCaptureAction {
        PostKeyCaptureAction::Fallthrough
    }
}

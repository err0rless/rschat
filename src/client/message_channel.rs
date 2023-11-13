use std::sync::{Arc, Mutex};

use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
    widgets::ListItem,
};

/// Thread safe queue for styled messages to be displayed on the message section
#[derive(Default, Clone)]
pub struct MessageChannel {
    pub messages: Arc<Mutex<Vec<(String, String)>>>,
}

impl MessageChannel {
    pub fn push(&self, id: String, msg: String) {
        self.messages.lock().unwrap().push((id, msg));
    }

    pub fn push_sys_msg(&mut self, msg: String) {
        self.push("System".to_owned(), msg);
    }

    pub fn push_sys_err(&mut self, msg: String) {
        self.push("SystemError".to_owned(), msg);
    }

    pub fn collect_list_item(&self) -> Vec<ListItem> {
        self.messages
            .lock()
            .unwrap()
            .iter()
            .map(|(id, msg)| {
                // construct a list of the styled items
                ListItem::new(match &id[..] {
                    "System" => Line::from(Span::styled(
                        format!("[System]: {}", msg),
                        Style::default().fg(Color::LightBlue),
                    )),
                    "SystemError" => Line::from(Span::styled(
                        format!("[SystemError]: {}", msg),
                        Style::default().fg(Color::LightRed),
                    )),
                    _ => Line::from(Span::raw(format!("{}: {}", id, msg))),
                })
            })
            .collect()
    }
}

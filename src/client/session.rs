const DEFAULT_ENTRY_CHANNEL: &str = "public";

/// Session state container for Client
#[derive(Debug, Clone)]
pub struct State {
    /// Current login user name
    pub id: String,

    /// Current channe name
    pub channel: String,

    /// True if you are a guest
    pub is_guest: bool,
}

impl State {
    /// User entered as a guest
    pub fn new_guest(id: &str) -> Self {
        State {
            id: id.to_owned(),
            channel: DEFAULT_ENTRY_CHANNEL.to_owned(),
            is_guest: true,
        }
    }
}

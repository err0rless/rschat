// Session state container for Client
#[derive(Debug, Clone)]
pub struct State {
    /// Current login user name
    pub id: String,

    /// Current channe name
    pub channel: String,

    /// True if you are a guest
    pub is_guest: bool,
}

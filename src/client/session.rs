// Session state container for Client
#[derive(Debug, Clone)]
pub struct State {
    pub id: String,
    pub is_guest: bool,
}

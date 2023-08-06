use std::collections::HashSet;

pub const NUM_MAX_USER: usize = 32;

pub struct Session {
    pub names: HashSet<String>,
    pub num_user: usize,
}

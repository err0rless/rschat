use std::collections::{HashMap, HashSet};

use mysql::*;
use rand::prelude::*;
use tokio::sync::broadcast;

use crate::packet::*;

pub const NUM_MAX_GUEST: usize = 64;
pub const NUM_MAX_USER: usize = 128;

/// The default channel you enter when connecting to the server
pub const DEFAULT_CHANNEL: &str = "public";

/// Reserved system channels
pub const SYSTEM_CHANNELS: [&str; 3] = [DEFAULT_CHANNEL, "main", "dev"];

#[derive(Debug)]
pub struct State {
    pub names: HashSet<String>,
    pub num_user: usize,
    pub num_guest: usize,
}

impl State {
    pub fn new() -> Self {
        Self {
            names: HashSet::new(),
            num_user: 0,
            num_guest: 0,
        }
    }
}

/// Individual chat channel
#[derive(Debug)]
pub struct Channel {
    pub channel: broadcast::Sender<PacketType>,
    pub state: State,

    /// True if this is one of system channels
    pub is_system: bool,
}

impl Channel {
    pub fn leave_user(&mut self, name: &str) {
        if name.starts_with("guest_") {
            self.state.num_guest -= 1;
        } else {
            self.state.num_user -= 1;
        }
        self.state.names.remove(name);
    }

    pub fn num_guest(&self) -> usize {
        self.state.num_guest
    }

    pub fn num_user(&self) -> usize {
        self.state.num_user
    }

    pub fn has_user(&self, user_name: &str) -> bool {
        self.state.names.contains(user_name)
    }

    pub fn user_list(&self) -> Vec<String> {
        self.state
            .names
            .iter()
            .map(String::from)
            .collect::<Vec<String>>()
    }

    pub fn add_connection(&mut self, user_name: &str) -> bool {
        if user_name.starts_with("guest_") {
            self.state.num_guest += 1;
        } else {
            self.state.num_user += 1;
        }
        self.state.names.insert(user_name.to_owned())
    }

    /// Add a new user connection to `self`
    pub fn connect_user(
        &mut self,
        req: &LoginReq,
        cur_id: &str,
        pool: Pool,
    ) -> Result<String, String> {
        // Account Login
        if self.num_user() >= NUM_MAX_USER {
            return Err("too many users".to_owned());
        }

        // validation of inputs was done before this packet reached here, but somehow it's broken
        if req.login_info.id.is_none() || req.login_info.password.is_none() {
            return Err("broken login packet".to_owned());
        }

        let res = req.login_info.login(pool.clone());
        if res.is_ok() {
            self.leave_user(cur_id);
            self.add_connection(req.login_info.id.as_ref().unwrap().as_str());
        }
        res
    }

    /// Add a new guest connection to `self`
    pub fn connect_guest(&mut self) -> Result<String, String> {
        if self.num_guest() >= NUM_MAX_GUEST {
            return Err("too many guests".to_owned());
        }

        // Generate a random guest name
        let mut rng = rand::thread_rng();
        let guest_id = loop {
            let random_id = format!("guest_{}", rng.gen::<u16>());

            // duplicate check
            if !self.has_user(&random_id) {
                break random_id;
            }
        };

        self.add_connection(guest_id.clone().as_str());
        Ok(guest_id)
    }
}

/// Collection of channels
#[derive(Debug)]
pub struct Channels {
    pub channels: HashMap<String, Channel>,
}

impl Channels {
    /// create a new `Channels` with default system channels
    pub fn with_system_channels() -> Self {
        let mut channels = Self {
            channels: HashMap::new(),
        };

        // create default system channels
        for sys_ch in SYSTEM_CHANNELS {
            channels
                .create_channel(sys_ch, true)
                .expect("failed to create a system channel");
        }
        channels
    }

    /// true if `name` is valid as a channel name
    fn is_valid(name: &str) -> bool {
        const MINIMUM_LEN: usize = 3;

        name.len() >= MINIMUM_LEN
            // First character of the channel name should be either alphabet or underbar
            && (name.starts_with('_') || name.chars().next().unwrap().is_ascii_alphabetic())
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    }

    /// create a channel and add it to the list
    pub fn create_channel(&mut self, name: &str, is_system: bool) -> Option<&Channel> {
        if !Self::is_valid(name) || self.channels.contains_key(name) {
            // The name is either invalid or duplicate
            None
        } else {
            let (sender, _) = broadcast::channel::<PacketType>(32);
            self.channels.insert(
                name.to_owned(),
                Channel {
                    channel: sender,
                    state: State::new(),
                    is_system,
                },
            );
            self.channels.get(name)
        }
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut Channel> {
        self.channels.get_mut(name)
    }

    pub fn get_channel(&self, name: &str) -> Option<broadcast::Sender<PacketType>> {
        self.channels.get(name).map(|c| c.channel.clone())
    }
}

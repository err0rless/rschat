use std::collections::{HashMap, HashSet};
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

    pub fn get_channel(&self, name: &str) -> Option<broadcast::Sender<PacketType>> {
        self.channels.get(name).map(|c| c.channel.clone())
    }
}

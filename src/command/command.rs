pub enum Command {
    Help,
    Exit,
}

impl Command {
    pub fn from_str(s: &str) -> Option<Command> {
        if !s.starts_with('/') {
            return None;
        }

        let command = if let Some(idx) = s.find(' ') {
            &s[1..idx]
        } else {
            &s[1..]
        };

        match command {
            "exit" => Some(Command::Exit),
            "help" => Some(Command::Help),
            cmd => {
                println!("Unknown command: '{}'", cmd);
                None
            }
        }
    }

    pub fn help() {
        println!(" | ----- Help -----");
        println!(" | /help: help message");
        println!(" | /exit: exit from chat");
    }
}

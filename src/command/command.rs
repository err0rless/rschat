pub enum Command {
    Help,
    Get(String),
    Exit,
}

impl Command {
    pub fn from_str(s: &str) -> Option<Command> {
        let cmdline = s.trim_end();
        if !cmdline.starts_with('/') {
            return None;
        }

        let command = if let Some(idx) = cmdline.find(' ') {
            &cmdline[1..idx]
        } else {
            &cmdline[1..]
        };

        match command {
            "exit" => Some(Command::Exit),
            "help" => Some(Command::Help),
            "get" => {
                if let Some(idx) = cmdline.find(' ') {
                    let item = String::from(cmdline[idx + 1..].trim());
                    Some(Command::Get(item))
                } else {
                    println!("[#SystemError] Command 'get' requires argument: 'get <key>'");
                    None
                }
            }
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

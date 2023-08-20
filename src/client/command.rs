// Request specific type of information from server
pub enum Fetch {
    UserList,
    None,
}

pub enum Command {
    Help,
    Get(String),
    Register,
    Login(Option<String>),
    Fetch(Fetch),
    Exit,
}

impl Command {
    pub fn from_str(s: &str) -> Option<Command> {
        if !s.starts_with('/') {
            return None;
        }

        let cmdline = s.trim_end();
        let command = if let Some(idx) = cmdline.find(' ') {
            &cmdline[1..idx]
        } else {
            &cmdline[1..]
        };

        match command {
            "exit" => Some(Command::Exit),
            "help" | "h" => Some(Command::Help),
            "register" | "reg" => Some(Command::Register),
            "login" => Some(Command::Login(
                cmdline
                    .find(' ')
                    .map(|idx| String::from(cmdline[idx + 1..].trim())),
            )),
            "get" => {
                if let Some(idx) = cmdline.find(' ') {
                    let item = String::from(cmdline[idx + 1..].trim());
                    Some(Command::Get(item))
                } else {
                    println!("[#SystemError] Command 'get' requires an argument: '[key]'");
                    None
                }
            }
            "fetch" => Some(Command::Fetch(
                match cmdline.find(' ').map(|idx| cmdline[idx + 1..].trim()) {
                    Some("list") => Fetch::UserList,
                    _ => Fetch::None,
                },
            )),
            cmd => {
                println!("Unknown command: '{}'", cmd);
                None
            }
        }
    }

    pub fn help() {
        println!(" | ----- Help -----");
        println!(" | /help: help message");
        println!(" | /register: register a new member");
        println!(" | /login <optional:id>: log in");
        println!(" | /get [required:key]: get information");
        println!(" | /exit: exit from chat");
    }
}

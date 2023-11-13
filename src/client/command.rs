use std::str::FromStr;

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
    Goto(String),
    Exit,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ParseCommandError {
    NotCommand,
    InvalidArgument(String),
    UnknownCommand(String),
}

impl FromStr for Command {
    type Err = ParseCommandError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if !s.starts_with('/') {
            return Err(ParseCommandError::NotCommand);
        }

        let cmdline = s.trim_end();
        let command = if let Some(idx) = cmdline.find(' ') {
            &cmdline[1..idx]
        } else {
            &cmdline[1..]
        };

        match command {
            "exit" => Ok(Command::Exit),
            "help" | "h" => Ok(Command::Help),
            "register" | "reg" => Ok(Command::Register),
            "login" => Ok(Command::Login(
                cmdline
                    .find(' ')
                    .map(|idx| String::from(cmdline[idx + 1..].trim())),
            )),
            "get" => {
                if let Some(idx) = cmdline.find(' ') {
                    let item = String::from(cmdline[idx + 1..].trim());
                    Ok(Command::Get(item))
                } else {
                    Err(ParseCommandError::InvalidArgument(
                        "Command 'get' requires an argument: '[key]'".to_owned(),
                    ))
                }
            }
            "fetch" => Ok(Command::Fetch(
                match cmdline.find(' ').map(|idx| cmdline[idx + 1..].trim()) {
                    Some("list") => Fetch::UserList,
                    _ => Fetch::None,
                },
            )),
            "goto" => match cmdline.find(' ') {
                Some(idx) => Ok(Command::Goto(String::from(cmdline[idx + 1..].trim()))),
                None => Err(ParseCommandError::InvalidArgument(
                    "[#SystemError] Command 'goto' requires an argument: [channel_name]".to_owned(),
                )),
            },
            unknown => Err(ParseCommandError::UnknownCommand(unknown.to_owned())),
        }
    }
}

impl Command {
    pub fn help() {
        println!(" | ----- Help -----");
        println!(" | /help: help message");
        println!(" | /register: register a new member");
        println!(" | /login <optional:id>: log in");
        println!(" | /get [required:key]: get information");
        println!(" | /goto [required:channel]: goto channel");
        println!(" | /exit: exit from chat");
    }
}

use std::fmt;

#[derive(Debug)]
pub enum ClientErr {
    JoinErr,
}

impl fmt::Display for ClientErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "ClientErr ouccurs!")?;
        Ok(())
    }
}

impl std::error::Error for ClientErr {
    fn description(&self) -> &str {
        "JoinError!"
    }
}

use std::io::Write;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tokio::io::AsyncBufReadExt;

use crate::crypto::hash;

fn print_flush(s: &str) {
    print!("{}", s);
    std::io::stdout().flush().unwrap();
}

async fn async_read_line() -> String {
    let mut buf: Vec<u8> = Vec::new();
    let mut reader = tokio::io::BufReader::new(tokio::io::stdin());

    _ = reader.read_until(b'\n', &mut buf).await;
    buf.pop();

    String::from_utf8(buf).unwrap()
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct User {
    pub id: String,
    pub password: String,
    pub bio: Option<String>,
    pub location: Option<String>,
}

impl User {
    pub async fn from_stdin() -> Option<Self> {
        print_flush(" - id: ");
        let id = async_read_line().await;

        print_flush(" - password: ");
        let password = async_read_line().await;

        print_flush(" - bio: ");
        let bio = async_read_line().await;

        print_flush(" - location: ");
        let loc = async_read_line().await;

        if id.is_empty() || password.is_empty() {
            println!("id or password is empty!");
            None
        } else {
            Some(Self {
                id,
                password: hash::sha256_password(&password),
                bio: if bio.is_empty() { None } else { Some(bio) },
                location: if loc.is_empty() { None } else { Some(loc) },
            })
        }
    }

    // check if self is valid
    pub fn insert(&self, sql: Arc<Mutex<rusqlite::Connection>>) -> Result<(), String> {
        if self.id.starts_with("guest_") || self.id.starts_with("root") {
            return Err("Reserved id format".to_owned());
        } else if self.password.len() < 4 {
            return Err("too short password! (password >= 4)".to_owned());
        }

        let conn = sql.lock();
        match conn.unwrap().execute(
            "INSERT INTO user (id, password, bio, location)
                VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                &self.id,
                &self.password,
                &self.bio.as_ref().unwrap_or(&"NULL".to_owned()),
                &self.location.as_ref().unwrap_or(&"NULL".to_owned()),
            ],
        ) {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("Failed to insert a new user: {}", e)),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Login {
    pub guest: bool,
    pub id: Option<String>,
    pub password: Option<String>,
}

impl Login {
    pub fn guest() -> Self {
        Self {
            guest: true,
            id: None,
            password: None,
        }
    }

    pub async fn from_stdin(id: Option<String>) -> Option<Self> {
        let id = if let Some(id) = id {
            id
        } else {
            print_flush(" - id: ");
            async_read_line().await
        };

        print_flush(" - password: ");
        let password = async_read_line().await;

        if id.is_empty() || password.is_empty() {
            None
        } else {
            Some(Self {
                guest: false,
                id: Some(id),
                password: Some(hash::sha256_password(&password)),
            })
        }
    }

    pub fn login(&self, sql: Arc<Mutex<rusqlite::Connection>>) -> Result<String, String> {
        if let Ok(conn) = sql.lock() {
            match conn.query_row(
                "SELECT id FROM user WHERE id=?1 AND password=?2",
                [&self.id, &self.password],
                |row| row.get::<_, String>(0),
            ) {
                Ok(r) => Ok(r),
                Err(e) => Err(e.to_string()),
            }
        } else {
            Err("Failed to lock sql connection".to_owned())
        }
    }
}

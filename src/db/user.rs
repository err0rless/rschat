use mysql::{prelude::*, *};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct User {
    pub id: String,
    pub password: String,
    pub bio: Option<String>,
    pub location: Option<String>,
}

impl User {
    // check if self is valid
    pub fn insert(&self, pool: Pool) -> Result<(), String> {
        if self.id.starts_with("guest_") || self.id.starts_with("root") {
            return Err("Reserved id format".to_owned());
        } else if self.password.len() < 4 {
            return Err("too short password! (password >= 4)".to_owned());
        }

        let mut conn = pool.get_conn().unwrap();
        match conn.exec_drop(
            "INSERT INTO user (id, password, bio, location) VALUES (:id, :password, :bio, :location)",
            params! {
                "id" => &self.id,
                "password" => &self.password,
                "bio" => &self.bio.as_ref().unwrap_or(&"NULL".to_owned()),
                "location" => &self.location.as_ref().unwrap_or(&"NULL".to_owned()),
            },
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

    pub fn login(&self, pool: Pool) -> Result<String, String> {
        if let Ok(mut conn) = pool.get_conn() {
            match conn.query_first::<String, _>(format!(
                "SELECT id FROM user WHERE id='{}' AND password='{}'",
                self.id.as_ref().unwrap(),
                self.password.as_ref().unwrap(),
            )) {
                Ok(Some(s)) => Ok(s),
                _ => Err("Wrong ID or Password".to_owned()),
            }
        } else {
            Err("Failed to get sql connection".to_owned())
        }
    }
}

use base64ct::{Base64, Encoding};
use sha2::{Digest, Sha256};

const PASSWORD_SALT: &str = "__simple_password_salt__";

/// hash string wtih SHA256 and encode the result in base64
pub fn sha256_string(s: &String) -> String {
    // SHA256 hashing
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    let result = hasher.finalize();

    // Base64 encoding
    Base64::encode_string(&result)
}

/// SHA256 hashing for `password` with custom hash salt
pub fn sha256_password(password: &str) -> String {
    // Append hash salt
    let salted_pw = password.to_owned() + PASSWORD_SALT;
    sha256_string(&salted_pw)
}

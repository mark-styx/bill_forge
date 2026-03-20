use argon2::{Algorithm, Argon2, Params, PasswordHasher, Version, password_hash::SaltString};
use rand::rngs::OsRng;

fn main() {
    let password = std::env::args().nth(1).unwrap_or_else(|| "password123".to_string());
    let salt = SaltString::generate(&mut OsRng);
    let params = Params::new(4096, 3, 1, None).unwrap();
    let hasher = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let hash = hasher.hash_password(password.as_bytes(), &salt).unwrap();
    println!("{}", hash);
}

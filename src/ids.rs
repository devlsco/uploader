use rand::Rng;

const ALPHABET: &[u8] = b"abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";

pub const RESERVED: &[&str] = &[
    "api", "_next", "a", "d", "login", "logout", "register", "setup", "files", "albums", "stats", "settings", "admin", "paste", "links", "view", "album", "delete", "qr",
    "index", "favicon.ico", "robots.txt", "icon.svg", "manifest.webmanifest", "health", "404",
];

pub fn new_id(len: usize) -> String {
    let mut rng = rand::thread_rng();
    loop {
        let id: String = (0..len).map(|_| ALPHABET[rng.gen_range(0..ALPHABET.len())] as char).collect();
        if !is_reserved(&id) {
            return id;
        }
    }
}

pub fn is_reserved(id: &str) -> bool {
    RESERVED.iter().any(|r| r.eq_ignore_ascii_case(id))
}

pub fn valid_custom(id: &str) -> bool {
    (3..=48).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') && !is_reserved(id)
}

pub fn valid_lookup(id: &str) -> bool {
    (1..=48).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids() {
        let id = new_id(6);
        assert_eq!(id.len(), 6);
        assert!(valid_lookup(&id));
        assert!(!valid_custom("api"));
        assert!(!valid_custom("a b"));
        assert!(valid_custom("my-link"));
    }
}

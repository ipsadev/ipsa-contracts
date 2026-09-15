use soroban_sdk::{panic_with_error, Env, String};

use crate::errors::Error;

const MIN_LEN: usize = 4;
const MAX_LEN: usize = 128;

const CHANNEL_PREFIX: &[u8] = b"channel-";
const CLIENT_PREFIX: &[u8] = b"client-";

pub(crate) fn validate_client_type(env: &Env, s: &String) {
    validate_custom_identifier(env, s);
}

pub(crate) fn validate_counterparty_client_id(env: &Env, s: &String) {
    validate_custom_identifier(env, s);
}

pub(crate) fn validate_port_id(env: &Env, s: &String) {
    validate_custom_identifier(env, s);
}

fn validate_custom_identifier(env: &Env, s: &String) {
    let len = s.len() as usize;
    if !(MIN_LEN..=MAX_LEN).contains(&len) {
        panic_with_error!(env, Error::InvalidIdentifierLength);
    }

    let mut buf = [0u8; MAX_LEN];
    s.copy_into_slice(&mut buf[..len]);
    let bytes = &buf[..len];

    if has_prefix(bytes, CHANNEL_PREFIX) || has_prefix(bytes, CLIENT_PREFIX) {
        panic_with_error!(env, Error::InvalidIdentifierPrefix);
    }

    for &c in bytes {
        if !is_allowed_char(c) {
            panic_with_error!(env, Error::InvalidIdentifierChar);
        }
    }
}

fn has_prefix(bz: &[u8], prefix: &[u8]) -> bool {
    bz.len() >= prefix.len() && &bz[..prefix.len()] == prefix
}

fn is_allowed_char(c: u8) -> bool {
    matches!(c,
        b'a'..=b'z'
            | b'A'..=b'Z'
            | b'0'..=b'9'
            | b'.'
            | b'_'
            | b'+'
            | b'-'
            | b'#'
            | b'['
            | b']'
            | b'<'
            | b'>'
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;

    fn s(env: &Env, value: &str) -> String {
        String::from_str(env, value)
    }

    #[test]
    fn accepts_typical_identifiers() {
        let env = Env::default();
        validate_custom_identifier(&env, &s(&env, "mock"));
        validate_custom_identifier(&env, &s(&env, "transfer"));
        validate_custom_identifier(&env, &s(&env, "10-stellar"));
        validate_custom_identifier(&env, &s(&env, "07-tendermint-0"));
        validate_custom_identifier(&env, &s(&env, "10-stellar-42"));
    }

    #[test]
    #[should_panic]
    fn rejects_too_short() {
        let env = Env::default();
        validate_custom_identifier(&env, &s(&env, "ab"));
    }

    #[test]
    #[should_panic]
    fn rejects_too_long() {
        let env = Env::default();
        let long: alloc::string::String = "a".repeat(129);
        validate_custom_identifier(&env, &s(&env, &long));
    }

    #[test]
    #[should_panic]
    fn rejects_channel_prefix() {
        let env = Env::default();
        validate_custom_identifier(&env, &s(&env, "channel-0"));
    }

    #[test]
    #[should_panic]
    fn rejects_client_prefix() {
        let env = Env::default();
        validate_custom_identifier(&env, &s(&env, "client-0"));
    }

    #[test]
    #[should_panic]
    fn rejects_space() {
        let env = Env::default();
        validate_custom_identifier(&env, &s(&env, "ab cd"));
    }

    #[test]
    #[should_panic]
    fn rejects_slash() {
        let env = Env::default();
        validate_custom_identifier(&env, &s(&env, "ab/cd"));
    }

    extern crate alloc;
}

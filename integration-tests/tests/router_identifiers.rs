use soroban_sdk::{Env, String};
use stellar_ibc_router::identifiers::validate_custom_identifier;

fn s(env: &Env, value: &str) -> String {
    String::from_str(env, value)
}

#[test]
fn accepts_typical_identifiers() {
    let env = Env::default();
    validate_custom_identifier(&env, &s(&env, "test"));
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
    let long = "a".repeat(129);
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

#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Bytes, Env, String};
use stellar_ibc_gmp::{IbcGmpApp, IbcGmpAppClient};

fn deploy(env: &Env) -> IbcGmpAppClient<'_> {
    let owner = Address::generate(env);
    let router = Address::generate(env);
    let id = env.register(IbcGmpApp, (owner, router));

    IbcGmpAppClient::new(env, &id)
}

#[test]
fn the_same_triplet_always_derives_the_same_address() {
    let env = Env::default();
    let app = deploy(&env);

    let client = String::from_str(&env, "07-tendermint-0");
    let sender = String::from_str(&env, "cosmos1abc");
    let salt = Bytes::from_array(&env, &[1u8; 4]);

    let first = app.account_address(&client, &sender, &salt);
    let second = app.account_address(&client, &sender, &salt);

    assert_eq!(first.address, second.address);
}

#[test]
fn a_different_salt_derives_a_different_address() {
    let env = Env::default();
    let app = deploy(&env);

    let client = String::from_str(&env, "07-tendermint-0");
    let sender = String::from_str(&env, "cosmos1abc");

    let one = app.account_address(&client, &sender, &Bytes::from_array(&env, &[1u8]));
    let two = app.account_address(&client, &sender, &Bytes::from_array(&env, &[2u8]));

    assert_ne!(one.address, two.address);
}

#[test]
fn a_different_client_derives_a_different_address() {
    let env = Env::default();
    let app = deploy(&env);

    let sender = String::from_str(&env, "cosmos1abc");
    let salt = Bytes::from_array(&env, &[1u8]);

    let one = app.account_address(&String::from_str(&env, "07-tendermint-0"), &sender, &salt);
    let two = app.account_address(&String::from_str(&env, "07-tendermint-1"), &sender, &salt);

    assert_ne!(one.address, two.address);
}

// Length prefixing is what stops ("ab", "c") and ("a", "bc") hashing alike.
#[test]
fn concatenation_boundaries_cannot_be_shifted() {
    let env = Env::default();
    let app = deploy(&env);

    let salt = Bytes::from_array(&env, &[0u8]);

    let one = app.account_address(
        &String::from_str(&env, "07-tendermint-0"),
        &String::from_str(&env, "aab"),
        &salt,
    );
    let two = app.account_address(
        &String::from_str(&env, "07-tendermint-0a"),
        &String::from_str(&env, "ab"),
        &salt,
    );

    assert_ne!(one.address, two.address);
}

#[test]
fn an_account_is_not_materialized_until_a_packet_creates_it() {
    let env = Env::default();
    let app = deploy(&env);

    let derived = app.account_address(
        &String::from_str(&env, "07-tendermint-0"),
        &String::from_str(&env, "cosmos1abc"),
        &Bytes::from_array(&env, &[1u8]),
    );

    assert!(!derived.materialized);
}

#[test]
fn the_port_and_version_match_the_spec() {
    let env = Env::default();
    let app = deploy(&env);

    assert_eq!(app.port(), String::from_str(&env, "gmpport"));
    assert_eq!(app.version(), String::from_str(&env, "ics27-2"));
}

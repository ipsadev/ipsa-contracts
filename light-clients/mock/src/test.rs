#![cfg(test)]

use super::*;
use soroban_sdk::{Bytes, Env, String};

#[test]
fn initialise_sets_state_and_height() {
    let env = Env::default();
    let contract_id = env.register(MockLightClient, ());
    let client = MockLightClientClient::new(&env, &contract_id);

    let client_id = String::from_str(&env, "10-stellar-0");
    let cs = Bytes::from_slice(&env, b"client-state");
    let cons = Bytes::from_slice(&env, b"consensus-state");
    client.initialise(&client_id, &cs, &cons, &42);

    assert_eq!(client.latest_height(&client_id), 42);
    assert_eq!(client.client_state(&client_id), cs);
    assert_eq!(client.consensus_state(&client_id, &42), cons);
}

#[test]
fn update_state_bumps_height() {
    let env = Env::default();
    let contract_id = env.register(MockLightClient, ());
    let client = MockLightClientClient::new(&env, &contract_id);

    let client_id = String::from_str(&env, "10-stellar-0");
    client.initialise(
        &client_id,
        &Bytes::from_slice(&env, b"cs"),
        &Bytes::from_slice(&env, b"cons"),
        &7,
    );

    let new_height = client.update_state(&client_id, &Bytes::from_slice(&env, b"msg"));
    assert_eq!(new_height, 8);
    assert_eq!(client.latest_height(&client_id), 8);
}

#[test]
fn membership_proofs_always_succeed() {
    let env = Env::default();
    let contract_id = env.register(MockLightClient, ());
    let client = MockLightClientClient::new(&env, &contract_id);

    let client_id = String::from_str(&env, "10-stellar-0");
    let any = Bytes::from_slice(&env, b"anything");
    assert!(client.verify_membership(&client_id, &1, &any, &any, &any));
    assert!(client.verify_non_membership(&client_id, &1, &any, &any));
}

#[test]
fn misbehaviour_freezes_client() {
    let env = Env::default();
    let contract_id = env.register(MockLightClient, ());
    let client = MockLightClientClient::new(&env, &contract_id);

    let client_id = String::from_str(&env, "10-stellar-0");
    client.initialise(
        &client_id,
        &Bytes::from_slice(&env, b"cs"),
        &Bytes::from_slice(&env, b"cons"),
        &0,
    );
    assert!(!client.frozen(&client_id));
    client.update_state_on_misbehaviour(&client_id, &Bytes::from_slice(&env, b"evidence"));
    assert!(client.frozen(&client_id));
}

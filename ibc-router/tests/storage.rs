mod common;

use common::setup;
use soroban_sdk::{BytesN, String};

#[test]
fn packet_commitment_round_trips_and_can_be_deleted() {
    let (env, _router_id, router, _lc_id) = setup();
    let client_id = String::from_str(&env, "10-stellar-0");
    let hash = BytesN::from_array(&env, &[0xAB; 32]);

    assert!(router.packet_commitment(&client_id, &7).is_none());
    router.set_packet_commitment(&client_id, &7, &hash);
    assert_eq!(router.packet_commitment(&client_id, &7).unwrap(), hash);

    router.delete_packet_commitment(&client_id, &7);
    assert!(router.packet_commitment(&client_id, &7).is_none());
}

#[test]
fn packet_receipt_round_trips() {
    let (env, _router_id, router, _lc_id) = setup();
    let client_id = String::from_str(&env, "10-stellar-0");

    assert!(!router.has_packet_receipt(&client_id, &3));
    router.set_packet_receipt(&client_id, &3);
    assert!(router.has_packet_receipt(&client_id, &3));
    assert!(!router.has_packet_receipt(&client_id, &4));
}

#[test]
fn ack_commitment_round_trips() {
    let (env, _router_id, router, _lc_id) = setup();
    let client_id = String::from_str(&env, "10-stellar-0");
    let ack = BytesN::from_array(&env, &[0xCD; 32]);

    assert!(router.acknowledgement(&client_id, &11).is_none());
    router.set_ack_commitment(&client_id, &11, &ack);
    assert_eq!(router.acknowledgement(&client_id, &11).unwrap(), ack);
}

#[test]
fn provable_paths_are_keyed_by_distinct_discriminators() {
    let (env, _router_id, router, _lc_id) = setup();
    let client_id = String::from_str(&env, "10-stellar-0");

    let same_value = BytesN::from_array(&env, &[0xAA; 32]);
    router.set_packet_commitment(&client_id, &1, &same_value);
    router.set_ack_commitment(&client_id, &1, &same_value);
    router.set_packet_receipt(&client_id, &1);

    assert_eq!(
        router.packet_commitment(&client_id, &1).unwrap(),
        same_value
    );
    assert_eq!(router.acknowledgement(&client_id, &1).unwrap(), same_value);
    assert!(router.has_packet_receipt(&client_id, &1));

    router.delete_packet_commitment(&client_id, &1);
    assert!(router.packet_commitment(&client_id, &1).is_none());
    assert_eq!(router.acknowledgement(&client_id, &1).unwrap(), same_value);
    assert!(router.has_packet_receipt(&client_id, &1));
}

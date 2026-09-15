mod common;

use common::{router_roots, setup};
use soroban_sdk::{BytesN, String};

fn zero() -> [u8; 32] {
    [0u8; 32]
}

#[test]
fn the_root_event_has_the_shape_the_light_client_expects() {
    let (env, router_id, router, _lc) = setup();
    let client_id = String::from_str(&env, "10-stellar-0");

    router.set_packet_commitment(&client_id, &7, &BytesN::from_array(&env, &[0xAB; 32]));

    let roots = router_roots(&env, &router_id);
    assert_eq!(roots.len(), 1, "exactly one root event per provable write");
    assert_ne!(
        roots[0],
        zero(),
        "a populated tree must not report the empty root"
    );
}

#[test]
fn every_provable_write_republishes_the_root() {
    let (env, router_id, router, _lc) = setup();
    let client_id = String::from_str(&env, "10-stellar-0");

    router.set_packet_commitment(&client_id, &1, &BytesN::from_array(&env, &[0x01; 32]));
    let after_commitment = *router_roots(&env, &router_id).last().unwrap();

    router.set_packet_receipt(&client_id, &1);
    let after_receipt = *router_roots(&env, &router_id).last().unwrap();
    assert_ne!(
        after_commitment, after_receipt,
        "a receipt is provable state and must move the root"
    );

    router.set_ack_commitment(&client_id, &1, &BytesN::from_array(&env, &[0x02; 32]));
    let after_ack = *router_roots(&env, &router_id).last().unwrap();
    assert_ne!(after_receipt, after_ack, "an ack must move the root");
}

#[test]
fn deleting_a_commitment_moves_the_root_back() {
    let (env, _router_id, router, _lc) = setup();
    let client_id = String::from_str(&env, "10-stellar-0");

    let empty = router.ibc_root();
    router.set_packet_commitment(&client_id, &7, &BytesN::from_array(&env, &[0xAB; 32]));
    let populated = router.ibc_root();
    assert_ne!(empty, populated);

    router.delete_packet_commitment(&client_id, &7);
    assert_eq!(
        router.ibc_root(),
        empty,
        "removing the only leaf returns to the empty root"
    );
}

#[test]
fn deleting_something_absent_still_publishes_one_unchanged_root() {
    let (env, router_id, router, _lc) = setup();
    let client_id = String::from_str(&env, "10-stellar-0");

    router.delete_packet_commitment(&client_id, &999);

    let roots = router_roots(&env, &router_id);
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0], zero(), "nothing was removed, so the root stands");
}

#[test]
fn commit_root_republishes_without_changing_state() {
    let (env, router_id, router, _lc) = setup();
    let client_id = String::from_str(&env, "10-stellar-0");
    router.set_packet_commitment(&client_id, &7, &BytesN::from_array(&env, &[0xAB; 32]));

    let before = router.ibc_root();
    let republished = router.commit_root();

    assert_eq!(
        before, republished,
        "an idle republish must not move the root"
    );

    let roots = router_roots(&env, &router_id);
    assert_eq!(roots.len(), 1, "the republish emits exactly one root event");
    assert_eq!(
        roots[0],
        before.to_array(),
        "and it carries the unchanged root"
    );
}

#[test]
fn an_untouched_router_reports_the_empty_root() {
    let (env, _router_id, router, _lc) = setup();
    assert_eq!(router.ibc_root(), BytesN::from_array(&env, &[0u8; 32]));
}

#[test]
fn the_root_tracks_the_stored_value() {
    let (env, _router_id, router, _lc) = setup();
    let client_id = String::from_str(&env, "10-stellar-0");

    router.set_packet_commitment(&client_id, &7, &BytesN::from_array(&env, &[0xAA; 32]));
    let first = router.ibc_root();

    router.set_packet_commitment(&client_id, &7, &BytesN::from_array(&env, &[0xBB; 32]));
    let second = router.ibc_root();

    assert_ne!(
        first, second,
        "overwriting a commitment with a different value must move the root"
    );
}

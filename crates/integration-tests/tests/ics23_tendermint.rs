use integration_tests::ics23::{
    absence_proof, b, exist_cp, keys, membership_fixture, merkle_bytes, non_membership_fixture,
    receipt_key, receipts, store_keys, STORE,
};
use soroban_sdk::{testutils::Address as _, xdr::ToXdr, Bytes, BytesN, Env, String};
use stellar_tendermint_light_client::{
    Height, TendermintClientState, TendermintConsensusState, TendermintLightClient,
    TendermintLightClientClient, TrustThreshold,
};

fn setup(env: &Env, root: [u8; 32], height: u64) -> (TendermintLightClientClient<'static>, String) {
    let router = soroban_sdk::Address::generate(env);
    let addr = env.register(TendermintLightClient, (router,));

    env.mock_all_auths();

    let client = TendermintLightClientClient::new(env, &addr);
    let client_id = String::from_str(env, "07-tendermint-0");

    let client_state = TendermintClientState {
        chain_id: String::from_str(env, "test-chain-0"),
        trust_level: TrustThreshold {
            numerator: 1,
            denominator: 3,
        },
        trusting_period_secs: 1_000_000,
        unbonding_period_secs: 2_000_000,
        max_clock_drift_secs: 10,
        latest_height: Height {
            revision_number: 0,
            revision_height: height,
        },
        is_frozen: false,
        frozen_height: Height {
            revision_number: 0,
            revision_height: 0,
        },
        proof_specs: Bytes::new(env),
    };
    let consensus_state = TendermintConsensusState {
        timestamp_secs: 1,
        next_validators_hash: BytesN::from_array(env, &[0u8; 32]),
        root: BytesN::from_array(env, &root),
    };

    client.initialise(
        &client_id,
        &client_state.to_xdr(env),
        &consensus_state.to_xdr(env),
        &height,
    );

    (client, client_id)
}

#[test]
fn the_client_has_two_proof_levels() {
    let env = Env::default();
    let (client, _) = setup(&env, [0u8; 32], 10);

    assert_eq!(client.proof_levels(), 2);
}

#[test]
fn membership_accepts_valid_two_layer_proof() {
    let env = Env::default();
    let f = membership_fixture();
    let (client, cid) = setup(&env, f.root, 10);

    assert!(client.verify_membership(
        &cid,
        &10,
        &b(&env, &f.proof),
        &store_keys(&env, &f.key),
        &b(&env, &f.value)
    ));
}

#[test]
fn membership_rejects_a_wrong_value() {
    let env = Env::default();
    let fixture = membership_fixture();
    let (client, client_id) = setup(&env, fixture.root, 10);

    let other_value = b(&env, &[0x02]);

    assert!(!client.verify_membership(
        &client_id,
        &10,
        &b(&env, &fixture.proof),
        &store_keys(&env, &fixture.key),
        &other_value
    ));
}

#[test]
fn membership_rejects_a_store_key_split_across_levels() {
    let env = Env::default();
    let f = membership_fixture();
    let (client, cid) = setup(&env, f.root, 10);

    let split_tail = [b"c".as_slice(), &f.key].concat();

    assert_eq!(
        [b"ib".as_slice(), &split_tail].concat(),
        [STORE, &f.key].concat()
    );
    assert!(!client.verify_membership(
        &cid,
        &10,
        &b(&env, &f.proof),
        &keys(&env, &[b"ib", &split_tail]),
        &b(&env, &f.value)
    ));
}

#[test]
fn membership_rejects_unknown_height() {
    let env = Env::default();
    let f = membership_fixture();
    let (client, cid) = setup(&env, f.root, 10);

    assert!(!client.verify_membership(
        &cid,
        &99,
        &b(&env, &f.proof),
        &store_keys(&env, &f.key),
        &b(&env, &f.value)
    ));
}

#[test]
fn membership_rejects_when_frozen() {
    let env = Env::default();
    let f = membership_fixture();
    let (client, cid) = setup(&env, f.root, 10);

    client.update_state_on_misbehaviour(&cid, &b(&env, b""));

    assert!(!client.verify_membership(
        &cid,
        &10,
        &b(&env, &f.proof),
        &store_keys(&env, &f.key),
        &b(&env, &f.value)
    ));
}

#[test]
fn non_membership_accepts_valid_absence_proof() {
    let env = Env::default();
    let f = non_membership_fixture();
    let (client, cid) = setup(&env, f.root, 10);

    assert!(client.verify_non_membership(&cid, &10, &b(&env, &f.proof), &store_keys(&env, &f.key)));
}

#[test]
fn non_membership_rejects_a_present_key_between_its_neighbours() {
    let env = Env::default();
    let committed = receipts();
    let present = receipt_key(3);
    let (client, client_id) = setup(&env, committed.multistore.root, 10);

    let membership = merkle_bytes(&[
        exist_cp(committed.tree.proofs[1].clone()),
        exist_cp(committed.multistore.proof.clone()),
    ]);

    assert!(
        client.verify_membership(
            &client_id,
            &10,
            &b(&env, &membership),
            &store_keys(&env, &present),
            &b(&env, &[0x01])
        ),
        "the client must hold this key for the absence claim below to mean anything"
    );

    let proof = absence_proof(&committed, &present, Some(0), Some(2));

    assert!(!client.verify_non_membership(
        &client_id,
        &10,
        &b(&env, &proof),
        &store_keys(&env, &present)
    ));
}

#[test]
fn non_membership_rejects_a_present_key_with_a_lone_left_neighbour() {
    let env = Env::default();
    let committed = receipts();
    let present = receipt_key(5);
    let (client, client_id) = setup(&env, committed.multistore.root, 10);

    assert_eq!(committed.keys[2], present);

    let proof = absence_proof(&committed, &present, Some(1), None);

    assert!(!client.verify_non_membership(
        &client_id,
        &10,
        &b(&env, &proof),
        &store_keys(&env, &present)
    ));
}

#[test]
fn non_membership_rejects_a_present_key_with_a_lone_right_neighbour() {
    let env = Env::default();
    let committed = receipts();
    let present = receipt_key(5);
    let (client, client_id) = setup(&env, committed.multistore.root, 10);

    assert_eq!(committed.keys[2], present);

    let proof = absence_proof(&committed, &present, None, Some(3));

    assert!(!client.verify_non_membership(
        &client_id,
        &10,
        &b(&env, &proof),
        &store_keys(&env, &present)
    ));
}

#[test]
fn non_membership_rejects_unknown_height() {
    let env = Env::default();
    let f = non_membership_fixture();
    let (client, cid) = setup(&env, f.root, 10);

    assert!(!client.verify_non_membership(
        &cid,
        &99,
        &b(&env, &f.proof),
        &store_keys(&env, &f.key)
    ));
}

#[test]
fn non_membership_rejects_when_frozen() {
    let env = Env::default();
    let f = non_membership_fixture();
    let (client, cid) = setup(&env, f.root, 10);

    client.update_state_on_misbehaviour(&cid, &b(&env, b""));

    assert!(!client.verify_non_membership(
        &cid,
        &10,
        &b(&env, &f.proof),
        &store_keys(&env, &f.key)
    ));
}

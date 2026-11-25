use integration_tests::tendermint::{Entry, HEIGHT};
use integration_tests::transfer::{inbound_packet_in, register_voucher, setup_committing, traced};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    vec, Address, Bytes,
};
use stellar_ibc_router::ics24::{commit_v2_acknowledgement, error_ack_hash};

fn error_acknowledgement(env: &soroban_sdk::Env) -> soroban_sdk::BytesN<32> {
    let error: Bytes = error_ack_hash(env).into();

    commit_v2_acknowledgement(env, &vec![env, error])
}

#[test]
fn a_zero_amount_transfer_is_answered_with_an_error_acknowledgement() {
    let env = soroban_sdk::Env::default();
    let receiver = Address::generate(&env);
    let packet = inbound_packet_in(&env, "ics20-1", &receiver, 0);
    let f = setup_committing(env, &[Entry::Packet(packet.clone())]);
    let voucher = traced(&f, "transfer", &f.source_client_id, "XLM");

    register_voucher(&f, &voucher);
    f.env.ledger().set_timestamp(1_000);
    f.router.recv_packet(&packet, &f.proofs[0].clone(), &HEIGHT);

    assert_eq!(
        f.router.acknowledgement(&f.source_client_id, &1),
        Some(error_acknowledgement(&f.env))
    );
    assert_eq!(f.transfer.balance_of(&receiver, &voucher), 0);
}

#[test]
fn a_negative_amount_transfer_is_answered_with_an_error_acknowledgement() {
    let env = soroban_sdk::Env::default();
    let receiver = Address::generate(&env);
    let packet = inbound_packet_in(&env, "ics20-1", &receiver, -5);
    let f = setup_committing(env, &[Entry::Packet(packet.clone())]);
    let voucher = traced(&f, "transfer", &f.source_client_id, "XLM");

    register_voucher(&f, &voucher);
    f.env.ledger().set_timestamp(1_000);
    f.router.recv_packet(&packet, &f.proofs[0].clone(), &HEIGHT);

    assert_eq!(
        f.router.acknowledgement(&f.source_client_id, &1),
        Some(error_acknowledgement(&f.env))
    );
}

#[test]
fn a_transfer_with_another_version_is_answered_with_an_error_acknowledgement() {
    let env = soroban_sdk::Env::default();
    let receiver = Address::generate(&env);
    let packet = inbound_packet_in(&env, "ics20-2", &receiver, 500);
    let f = setup_committing(env, &[Entry::Packet(packet.clone())]);
    let voucher = traced(&f, "transfer", &f.source_client_id, "XLM");

    register_voucher(&f, &voucher);
    f.env.ledger().set_timestamp(1_000);
    f.router.recv_packet(&packet, &f.proofs[0].clone(), &HEIGHT);

    assert_eq!(
        f.router.acknowledgement(&f.source_client_id, &1),
        Some(error_acknowledgement(&f.env))
    );
    assert_eq!(f.transfer.balance_of(&receiver, &voucher), 0);
}

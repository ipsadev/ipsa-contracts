use integration_tests::tendermint::{Entry, HEIGHT};
use integration_tests::transfer::{
    fund, inbound_packet_in, outbound_packet_in, register_local, register_voucher, setup,
    setup_committing, traced, xlm,
};
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    vec, Address, Bytes, Event, String,
};
use stellar_ibc_transfer::{TransferCredited, TransferRefunded, TransferSent};

#[test]
fn sending_publishes_transfer_sent() {
    let f = setup();
    let token = register_local(&f, &xlm(&f.env));
    let sender = Address::generate(&f.env);
    let receiver = String::from_str(&f.env, "cosmos1abc");

    fund(&f, &token, &sender, 1_000);

    let sequence = f.transfer.initiate_transfer(
        &sender,
        &f.source_client_id,
        &xlm(&f.env),
        &400,
        &receiver,
        &2_000,
        &String::from_str(&f.env, ""),
    );
    let sent = TransferSent {
        source_client: f.source_client_id.clone(),
        sequence,
        sender,
        denom: xlm(&f.env),
        amount: 400,
        receiver,
    };

    assert_eq!(
        f.env.events().all().filter_by_contract(&f.transfer_addr),
        [sent.to_xdr(&f.env, &f.transfer_addr)]
    );
}

#[test]
fn receiving_publishes_transfer_credited() {
    let env = soroban_sdk::Env::default();
    let receiver = Address::generate(&env);
    let packet = inbound_packet_in(&env, "ics20-1", &receiver, 500);
    let f = setup_committing(env, &[Entry::Packet(packet.clone())]);
    let voucher = traced(&f, "transfer", &f.source_client_id, "XLM");

    register_voucher(&f, &voucher);
    f.env.ledger().set_timestamp(1_000);
    f.router.recv_packet(&packet, &f.proofs[0].clone(), &HEIGHT);

    let credited = TransferCredited {
        dest_client: f.source_client_id.clone(),
        sequence: 1,
        sender: String::from_str(&f.env, "cosmos1abc"),
        receiver,
        denom: voucher,
        amount: 500,
    };

    assert!(f
        .env
        .events()
        .all()
        .filter_by_contract(&f.transfer_addr)
        .events()
        .contains(&credited.to_xdr(&f.env, &f.transfer_addr)));
}

#[test]
fn an_error_acknowledgement_publishes_transfer_refunded() {
    let env = soroban_sdk::Env::default();
    let sender = Address::generate(&env);
    let outbound = outbound_packet_in(&env, 1, &sender, 400);
    let acks = vec![&env, Bytes::from_slice(&env, b"\xff\xff")];
    let f = setup_committing(env, &[Entry::Acknowledgement(outbound.clone(), acks)]);
    let token = register_local(&f, &xlm(&f.env));

    fund(&f, &token, &sender, 1_000);

    let sequence = f.transfer.initiate_transfer(
        &sender,
        &f.source_client_id,
        &xlm(&f.env),
        &400,
        &String::from_str(&f.env, "cosmos1abc"),
        &2_000,
        &String::from_str(&f.env, ""),
    );

    f.router.acknowledge_packet(
        &outbound,
        &vec![&f.env, Bytes::from_slice(&f.env, b"\xff\xff")],
        &f.proofs[0].clone(),
        &HEIGHT,
    );

    let refunded = TransferRefunded {
        source_client: f.source_client_id.clone(),
        sequence,
        sender,
        denom: xlm(&f.env),
        amount: 400,
    };

    assert_eq!(
        f.env.events().all().filter_by_contract(&f.transfer_addr),
        [refunded.to_xdr(&f.env, &f.transfer_addr)]
    );
}

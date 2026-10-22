mod common;

use common::{
    fund, inbound_packet, outbound_packet, register_local, register_voucher, setup, traced, xlm,
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
    let f = setup();
    let voucher = traced(&f, "transfer", &f.source_client_id, "XLM");
    let receiver = Address::generate(&f.env);

    register_voucher(&f, &voucher);
    f.env.ledger().set_timestamp(1_000);
    f.router.recv_packet(
        &inbound_packet(&f, "ics20-1", &receiver, 500),
        &Bytes::from_slice(&f.env, b"proof"),
        &10,
    );

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
    let f = setup();
    let token = register_local(&f, &xlm(&f.env));
    let sender = Address::generate(&f.env);

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
        &outbound_packet(&f, sequence, &sender, 400),
        &vec![&f.env, Bytes::from_slice(&f.env, b"\xff\xff")],
        &Bytes::from_slice(&f.env, b"proof"),
        &10,
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

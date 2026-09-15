mod common;

use common::{
    build_inbound_packet, fund, register_local, register_voucher, setup, xlm, ENCODING,
    SUCCESS_ACK_BYTE,
};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{vec, Address, Bytes, String};
use stellar_ibc_transfer::{
    address_to_string, encode_ics20_json, Error, FungibleTokenPacketData, OnTimeoutPacketCallback,
    Payload, Token,
};

#[test]
fn initiate_transfer_escrows_and_emits_packet() {
    let f = setup();
    let sac = register_local(&f, &xlm(&f.env));
    let sender = Address::generate(&f.env);
    fund(&f, &sac, &sender, 1_000);

    let seq = f.transfer.initiate_transfer(
        &sender,
        &f.source_client_id,
        &xlm(&f.env),
        &250,
        &String::from_str(&f.env, "cosmos1abc"),
        &2_000,
        &String::from_str(&f.env, ""),
    );
    assert_eq!(seq, 1);
    assert_eq!(f.transfer.balance_of(&sender, &xlm(&f.env)), 750);
    assert_eq!(f.transfer.balance_of(&f.transfer_addr, &xlm(&f.env)), 250);

    assert!(f
        .router
        .packet_commitment(&f.source_client_id, &seq)
        .is_some());
}

#[test]
#[should_panic]
fn initiate_transfer_rejects_insufficient_balance() {
    let f = setup();
    let sac = register_local(&f, &xlm(&f.env));
    let sender = Address::generate(&f.env);
    fund(&f, &sac, &sender, 100);

    f.transfer.initiate_transfer(
        &sender,
        &f.source_client_id,
        &xlm(&f.env),
        &500,
        &String::from_str(&f.env, "cosmos1abc"),
        &2_000,
        &String::from_str(&f.env, ""),
    );
}

#[test]
fn initiate_transfer_rejects_zero_amount() {
    let f = setup();
    let sender = Address::generate(&f.env);

    let result = f.transfer.try_initiate_transfer(
        &sender,
        &f.source_client_id,
        &xlm(&f.env),
        &0,
        &String::from_str(&f.env, "cosmos1abc"),
        &2_000,
        &String::from_str(&f.env, ""),
    );
    assert_eq!(result, Err(Ok(Error::AmountMustBePositive.into())));
}

#[test]
fn recv_packet_mints_voucher_and_returns_success_ack() {
    let f = setup();
    register_voucher(&f, &xlm(&f.env));
    let receiver = Address::generate(&f.env);

    let packet = stellar_ibc_router::Packet {
        sequence: 1,
        source_client: f.counterparty_client_id.clone(),
        dest_client: f.source_client_id.clone(),
        timeout_timestamp: 2_000,
        payloads: vec![
            &f.env,
            stellar_ibc_router::Payload {
                source_port: String::from_str(&f.env, "transfer"),
                dest_port: String::from_str(&f.env, "transfer"),
                version: String::from_str(&f.env, "ics20-1"),
                encoding: String::from_str(&f.env, ENCODING),
                value: build_inbound_packet(&f.env, "cosmos1abc", &receiver, &xlm(&f.env), 500i128),
            },
        ],
    };

    f.env.ledger().set_timestamp(1_000);
    f.router
        .recv_packet(&packet, &Bytes::from_slice(&f.env, b"proof"), &10);

    assert_eq!(f.transfer.balance_of(&receiver, &xlm(&f.env)), 500);
    assert!(f.router.acknowledgement(&f.source_client_id, &1).is_some());
}

#[test]
fn recv_packet_releases_escrow_for_returning_native_asset() {
    let f = setup();
    let sac = register_local(&f, &xlm(&f.env));
    let receiver = Address::generate(&f.env);
    fund(&f, &sac, &f.transfer_addr, 500);

    let packet = stellar_ibc_router::Packet {
        sequence: 1,
        source_client: f.counterparty_client_id.clone(),
        dest_client: f.source_client_id.clone(),
        timeout_timestamp: 2_000,
        payloads: vec![
            &f.env,
            stellar_ibc_router::Payload {
                source_port: String::from_str(&f.env, "transfer"),
                dest_port: String::from_str(&f.env, "transfer"),
                version: String::from_str(&f.env, "ics20-1"),
                encoding: String::from_str(&f.env, ENCODING),
                value: build_inbound_packet(&f.env, "cosmos1abc", &receiver, &xlm(&f.env), 300i128),
            },
        ],
    };

    f.env.ledger().set_timestamp(1_000);
    f.router
        .recv_packet(&packet, &Bytes::from_slice(&f.env, b"proof"), &10);

    assert_eq!(f.transfer.balance_of(&receiver, &xlm(&f.env)), 300);
    assert_eq!(f.transfer.balance_of(&f.transfer_addr, &xlm(&f.env)), 200);
}

#[test]
fn acknowledge_packet_with_error_ack_refunds_sender() {
    let f = setup();
    let sac = register_local(&f, &xlm(&f.env));
    let sender = Address::generate(&f.env);
    fund(&f, &sac, &sender, 1_000);
    let seq = f.transfer.initiate_transfer(
        &sender,
        &f.source_client_id,
        &xlm(&f.env),
        &400,
        &String::from_str(&f.env, "cosmos1abc"),
        &2_000,
        &String::from_str(&f.env, ""),
    );
    assert_eq!(f.transfer.balance_of(&sender, &xlm(&f.env)), 600);
    assert_eq!(f.transfer.balance_of(&f.transfer_addr, &xlm(&f.env)), 400);

    let packet = stellar_ibc_router::Packet {
        sequence: seq,
        source_client: f.source_client_id.clone(),
        dest_client: f.counterparty_client_id.clone(),
        timeout_timestamp: 2_000,
        payloads: vec![
            &f.env,
            stellar_ibc_router::Payload {
                source_port: String::from_str(&f.env, "transfer"),
                dest_port: String::from_str(&f.env, "transfer"),
                version: String::from_str(&f.env, "ics20-1"),
                encoding: String::from_str(&f.env, ENCODING),
                value: {
                    let pkt = FungibleTokenPacketData {
                        token: Token {
                            denom: xlm(&f.env),
                            amount: 400,
                        },
                        sender: address_to_string(&f.env, &sender),
                        receiver: String::from_str(&f.env, "cosmos1abc"),
                        memo: String::from_str(&f.env, ""),
                    };
                    encode_ics20_json(&f.env, &pkt)
                },
            },
        ],
    };

    let err_ack = Bytes::from_slice(&f.env, b"\xff\xff");
    f.router.acknowledge_packet(
        &packet,
        &vec![&f.env, err_ack],
        &Bytes::from_slice(&f.env, b"proof"),
        &10,
    );

    assert_eq!(f.transfer.balance_of(&sender, &xlm(&f.env)), 1_000);
    assert_eq!(f.transfer.balance_of(&f.transfer_addr, &xlm(&f.env)), 0);
}

#[test]
fn acknowledge_packet_with_success_ack_leaves_escrow_locked() {
    let f = setup();
    let sac = register_local(&f, &xlm(&f.env));
    let sender = Address::generate(&f.env);
    fund(&f, &sac, &sender, 1_000);
    let seq = f.transfer.initiate_transfer(
        &sender,
        &f.source_client_id,
        &xlm(&f.env),
        &400,
        &String::from_str(&f.env, "cosmos1abc"),
        &2_000,
        &String::from_str(&f.env, ""),
    );

    let packet = stellar_ibc_router::Packet {
        sequence: seq,
        source_client: f.source_client_id.clone(),
        dest_client: f.counterparty_client_id.clone(),
        timeout_timestamp: 2_000,
        payloads: vec![
            &f.env,
            stellar_ibc_router::Payload {
                source_port: String::from_str(&f.env, "transfer"),
                dest_port: String::from_str(&f.env, "transfer"),
                version: String::from_str(&f.env, "ics20-1"),
                encoding: String::from_str(&f.env, ENCODING),
                value: {
                    let pkt = FungibleTokenPacketData {
                        token: Token {
                            denom: xlm(&f.env),
                            amount: 400,
                        },
                        sender: address_to_string(&f.env, &sender),
                        receiver: String::from_str(&f.env, "cosmos1abc"),
                        memo: String::from_str(&f.env, ""),
                    };
                    encode_ics20_json(&f.env, &pkt)
                },
            },
        ],
    };

    let success_ack = Bytes::from_slice(&f.env, &[SUCCESS_ACK_BYTE]);
    f.router.acknowledge_packet(
        &packet,
        &vec![&f.env, success_ack],
        &Bytes::from_slice(&f.env, b"proof"),
        &10,
    );

    assert_eq!(f.transfer.balance_of(&sender, &xlm(&f.env)), 600);
    assert_eq!(f.transfer.balance_of(&f.transfer_addr, &xlm(&f.env)), 400);
}

#[test]
fn timeout_packet_refunds_sender() {
    let f = setup();
    let sac = register_local(&f, &xlm(&f.env));
    let sender = Address::generate(&f.env);
    fund(&f, &sac, &sender, 1_000);
    let seq = f.transfer.initiate_transfer(
        &sender,
        &f.source_client_id,
        &xlm(&f.env),
        &400,
        &String::from_str(&f.env, "cosmos1abc"),
        &2_000,
        &String::from_str(&f.env, ""),
    );
    assert_eq!(f.transfer.balance_of(&f.transfer_addr, &xlm(&f.env)), 400);

    let callback = OnTimeoutPacketCallback {
        source_client: f.source_client_id.clone(),
        dest_client: f.counterparty_client_id.clone(),
        sequence: seq,
        payload: Payload {
            source_port: String::from_str(&f.env, "transfer"),
            dest_port: String::from_str(&f.env, "transfer"),
            version: String::from_str(&f.env, "ics20-1"),
            encoding: String::from_str(&f.env, ENCODING),
            value: {
                let pkt = FungibleTokenPacketData {
                    token: Token {
                        denom: xlm(&f.env),
                        amount: 400,
                    },
                    sender: address_to_string(&f.env, &sender),
                    receiver: String::from_str(&f.env, "cosmos1abc"),
                    memo: String::from_str(&f.env, ""),
                };
                encode_ics20_json(&f.env, &pkt)
            },
        },
    };

    f.transfer.on_timeout_packet(&callback);

    assert_eq!(f.transfer.balance_of(&sender, &xlm(&f.env)), 1_000);
    assert_eq!(f.transfer.balance_of(&f.transfer_addr, &xlm(&f.env)), 0);
}

use integration_tests::tendermint::{Entry, HEIGHT};
use integration_tests::transfer::{
    build_inbound_packet, fund, inbound_packet_in, outbound_packet_in, register_local,
    register_voucher, setup, setup_committing, traced, traced_in, xlm, COUNTERPARTY_CLIENT_ID,
    ENCODING, LOCAL_CLIENT_ID, SUCCESS_ACK_BYTE,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    vec, Address, Bytes, String,
};
use stellar_ibc_transfer::{
    address_to_string, encode_ics20_json, Error, FungibleTokenPacketData, OnTimeoutPacketCallback,
    Payload, Token,
};

#[test]
fn initiate_transfer_escrows_and_emits_packet() {
    let f = setup();
    let stellar_asset_contract = register_local(&f, &xlm(&f.env));
    let sender = Address::generate(&f.env);
    fund(&f, &stellar_asset_contract, &sender, 1_000);

    let sequence_number = f.transfer.initiate_transfer(
        &sender,
        &f.source_client_id,
        &xlm(&f.env),
        &250,
        &String::from_str(&f.env, "cosmos1abc"),
        &2_000,
        &String::from_str(&f.env, ""),
    );
    assert_eq!(sequence_number, 1);
    assert_eq!(f.transfer.balance_of(&sender, &xlm(&f.env)), 750);
    assert_eq!(f.transfer.balance_of(&f.transfer_addr, &xlm(&f.env)), 250);

    assert!(f
        .router
        .packet_commitment(&f.source_client_id, &sequence_number)
        .is_some());
}

#[test]
#[should_panic]
fn initiate_transfer_rejects_insufficient_balance() {
    let f = setup();
    let stellar_asset_contract = register_local(&f, &xlm(&f.env));
    let sender = Address::generate(&f.env);
    fund(&f, &stellar_asset_contract, &sender, 100);

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
    let env = soroban_sdk::Env::default();
    let receiver = Address::generate(&env);
    let packet = inbound_packet_in(&env, "ics20-1", &receiver, 500);
    let f = setup_committing(env, &[Entry::Packet(packet.clone())]);
    let voucher_denom = traced(&f, "transfer", &f.source_client_id, "XLM");
    register_voucher(&f, &voucher_denom);

    f.env.ledger().set_timestamp(1_000);
    f.router.recv_packet(&packet, &f.proofs[0].clone(), &HEIGHT);

    assert_eq!(f.transfer.balance_of(&receiver, &voucher_denom), 500);
    assert!(f.router.acknowledgement(&f.source_client_id, &1).is_some());
}

#[test]
fn recv_packet_releases_escrow_for_returning_native_asset() {
    let env = soroban_sdk::Env::default();
    let receiver = Address::generate(&env);
    let packet = stellar_ibc_router::Packet {
        sequence: 1,
        source_client: String::from_str(&env, COUNTERPARTY_CLIENT_ID),
        dest_client: String::from_str(&env, LOCAL_CLIENT_ID),
        timeout_timestamp: 2_000,
        payloads: vec![
            &env,
            stellar_ibc_router::Payload {
                source_port: String::from_str(&env, "transfer"),
                dest_port: String::from_str(&env, "transfer"),
                version: String::from_str(&env, "ics20-1"),
                encoding: String::from_str(&env, ENCODING),
                value: build_inbound_packet(
                    &env,
                    "cosmos1abc",
                    &receiver,
                    &traced_in(&env, "transfer", COUNTERPARTY_CLIENT_ID, "XLM"),
                    300i128,
                ),
            },
        ],
    };
    let f = setup_committing(env, &[Entry::Packet(packet.clone())]);
    let stellar_asset_contract = register_local(&f, &xlm(&f.env));
    fund(&f, &stellar_asset_contract, &f.transfer_addr, 500);

    f.env.ledger().set_timestamp(1_000);
    f.router.recv_packet(&packet, &f.proofs[0].clone(), &HEIGHT);

    assert_eq!(f.transfer.balance_of(&receiver, &xlm(&f.env)), 300);
    assert_eq!(f.transfer.balance_of(&f.transfer_addr, &xlm(&f.env)), 200);
}

#[test]
fn acknowledge_packet_with_error_ack_refunds_sender() {
    let env = soroban_sdk::Env::default();
    let sender = Address::generate(&env);
    let packet = outbound_packet_in(&env, 1, &sender, 400);
    let committed_acks = vec![&env, Bytes::from_slice(&env, b"\xff\xff")];
    let f = setup_committing(
        env,
        &[Entry::Acknowledgement(packet.clone(), committed_acks)],
    );
    let stellar_asset_contract = register_local(&f, &xlm(&f.env));
    fund(&f, &stellar_asset_contract, &sender, 1_000);
    let _sequence_number = f.transfer.initiate_transfer(
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

    let err_ack = Bytes::from_slice(&f.env, b"\xff\xff");
    f.router.acknowledge_packet(
        &packet,
        &vec![&f.env, err_ack],
        &f.proofs[0].clone(),
        &HEIGHT,
    );

    assert_eq!(f.transfer.balance_of(&sender, &xlm(&f.env)), 1_000);
    assert_eq!(f.transfer.balance_of(&f.transfer_addr, &xlm(&f.env)), 0);
}

#[test]
fn acknowledge_packet_with_success_ack_leaves_escrow_locked() {
    let env = soroban_sdk::Env::default();
    let sender = Address::generate(&env);
    let packet = outbound_packet_in(&env, 1, &sender, 400);
    let committed_acks = vec![&env, Bytes::from_slice(&env, &[SUCCESS_ACK_BYTE])];
    let f = setup_committing(
        env,
        &[Entry::Acknowledgement(packet.clone(), committed_acks)],
    );
    let stellar_asset_contract = register_local(&f, &xlm(&f.env));
    fund(&f, &stellar_asset_contract, &sender, 1_000);
    let _sequence_number = f.transfer.initiate_transfer(
        &sender,
        &f.source_client_id,
        &xlm(&f.env),
        &400,
        &String::from_str(&f.env, "cosmos1abc"),
        &2_000,
        &String::from_str(&f.env, ""),
    );

    let success_ack = Bytes::from_slice(&f.env, &[SUCCESS_ACK_BYTE]);
    f.router.acknowledge_packet(
        &packet,
        &vec![&f.env, success_ack],
        &f.proofs[0].clone(),
        &HEIGHT,
    );

    assert_eq!(f.transfer.balance_of(&sender, &xlm(&f.env)), 600);
    assert_eq!(f.transfer.balance_of(&f.transfer_addr, &xlm(&f.env)), 400);
}

#[test]
fn timeout_packet_refunds_sender() {
    let f = setup();
    let stellar_asset_contract = register_local(&f, &xlm(&f.env));
    let sender = Address::generate(&f.env);
    fund(&f, &stellar_asset_contract, &sender, 1_000);
    let sequence_number = f.transfer.initiate_transfer(
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
        sequence: sequence_number,
        payload: Payload {
            source_port: String::from_str(&f.env, "transfer"),
            dest_port: String::from_str(&f.env, "transfer"),
            version: String::from_str(&f.env, "ics20-1"),
            encoding: String::from_str(&f.env, ENCODING),
            value: {
                let packet_data = FungibleTokenPacketData {
                    token: Token {
                        denom: xlm(&f.env),
                        amount: 400,
                    },
                    sender: address_to_string(&f.env, &sender),
                    receiver: String::from_str(&f.env, "cosmos1abc"),
                    memo: String::from_str(&f.env, ""),
                };
                encode_ics20_json(&f.env, &packet_data)
            },
        },
    };

    f.transfer.on_timeout_packet(&callback);

    assert_eq!(f.transfer.balance_of(&sender, &xlm(&f.env)), 1_000);
    assert_eq!(f.transfer.balance_of(&f.transfer_addr, &xlm(&f.env)), 0);
}

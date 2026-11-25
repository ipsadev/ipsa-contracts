use integration_tests::tendermint::{commit_entries, register, Entry, HEIGHT};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::StellarAssetClient,
    vec, Address, Bytes, Env, String,
};
use stellar_ibc_router::{IbcRouter, IbcRouterClient, Packet, Payload};
use stellar_ibc_transfer::{
    address_to_string, encode_ics20_json, FungibleTokenPacketData, IbcTransferApp,
    IbcTransferAppClient, Token,
};

const SUCCESS_ACK_BYTE: u8 = 0x01;
const SOURCE_VERSION: &str = "ics20-1";
const SOURCE_ENCODING: &str = "application/json";

struct Net {
    env: Env,
    proofs: std::vec::Vec<Bytes>,
    router: IbcRouterClient<'static>,
    transfer: IbcTransferAppClient<'static>,
    transfer_addr: Address,
    client_id: String,
    counterparty_id: String,
}

fn xlm(env: &Env) -> String {
    String::from_str(env, "XLM")
}

fn register_local(net: &Net) -> Address {
    let issuer = Address::generate(&net.env);
    let stellar_asset_contract = net.env.register_stellar_asset_contract_v2(issuer).address();
    net.transfer
        .register_asset(&xlm(&net.env), &stellar_asset_contract, &false);

    stellar_asset_contract
}

fn voucher_denom(net: &Net) -> String {
    let mut traced = std::string::String::from("transfer/");

    traced.push_str(&net.client_id.to_string());
    traced.push_str("/XLM");

    String::from_str(&net.env, &traced)
}

fn register_voucher(net: &Net) {
    let stellar_asset_contract = net
        .env
        .register_stellar_asset_contract_v2(net.transfer_addr.clone())
        .address();

    net.transfer
        .register_asset(&voucher_denom(net), &stellar_asset_contract, &true);
}

fn fund(net: &Net, stellar_asset_contract: &Address, to: &Address, amount: i128) {
    StellarAssetClient::new(&net.env, stellar_asset_contract).mint(to, &amount);
}

fn setup() -> Net {
    setup_committing(Env::default(), &[])
}

fn setup_committing(env: Env, entries: &[Entry]) -> Net {
    env.mock_all_auths();

    let router_admin = Address::generate(&env);
    let router_addr = env.register(IbcRouter, (router_admin,));
    let router = IbcRouterClient::new(&env, &router_addr);

    let committed = commit_entries(&env, entries);
    let (_light_client, client_id) = register(&env, &router, "test", committed.root, 1);
    let counterparty_id = String::from_str(&env, "07-tendermint-0");
    router.register_counterparty(
        &client_id,
        &counterparty_id,
        &vec![&env, Bytes::from_slice(&env, b"ibc"), Bytes::new(&env)],
    );

    let transfer_admin = Address::generate(&env);
    let transfer_addr = env.register(IbcTransferApp, (router_addr.clone(), transfer_admin));
    let transfer = IbcTransferAppClient::new(&env, &transfer_addr);
    router.register_port(&String::from_str(&env, "transfer"), &transfer_addr);

    Net {
        env,
        proofs: committed.proofs,
        router,
        transfer,
        transfer_addr,
        client_id,
        counterparty_id,
    }
}

fn outbound_packet(net: &Net, sender: &Address, amount: i128, sequence: u64) -> Packet {
    let packet_data = FungibleTokenPacketData {
        token: Token {
            denom: xlm(&net.env),
            amount,
        },
        sender: address_to_string(&net.env, sender),
        receiver: String::from_str(&net.env, "cosmos1abc"),
        memo: String::from_str(&net.env, ""),
    };

    Packet {
        sequence,
        source_client: net.client_id.clone(),
        dest_client: net.counterparty_id.clone(),
        timeout_timestamp: 2_000,
        payloads: vec![
            &net.env,
            Payload {
                source_port: String::from_str(&net.env, "transfer"),
                dest_port: String::from_str(&net.env, "transfer"),
                version: String::from_str(&net.env, SOURCE_VERSION),
                encoding: String::from_str(&net.env, SOURCE_ENCODING),
                value: encode_ics20_json(&net.env, &packet_data),
            },
        ],
    }
}

#[test]
fn stellar_to_cosmos_round_trip_with_success_ack() {
    let env = Env::default();
    let sender = Address::generate(&env);
    let outbound = outbound_packet_in(&env, &sender, 400, 1);
    let acks = vec![&env, Bytes::from_slice(&env, &[SUCCESS_ACK_BYTE])];
    let net = setup_committing(env, &[Entry::Acknowledgement(outbound, acks)]);
    let stellar_asset_contract = register_local(&net);
    fund(&net, &stellar_asset_contract, &sender, 1_000);

    let sequence_number = net.transfer.initiate_transfer(
        &sender,
        &net.client_id,
        &xlm(&net.env),
        &400,
        &String::from_str(&net.env, "cosmos1abc"),
        &2_000,
        &String::from_str(&net.env, ""),
    );
    assert_eq!(net.transfer.balance_of(&sender, &xlm(&net.env)), 600);
    assert_eq!(
        net.transfer.balance_of(&net.transfer_addr, &xlm(&net.env)),
        400
    );
    assert!(net
        .router
        .packet_commitment(&net.client_id, &sequence_number)
        .is_some());

    let packet = outbound_packet(&net, &sender, 400, sequence_number);
    net.router.acknowledge_packet(
        &packet,
        &vec![&net.env, Bytes::from_slice(&net.env, &[SUCCESS_ACK_BYTE])],
        &net.proofs[0].clone(),
        &HEIGHT,
    );

    assert_eq!(net.transfer.balance_of(&sender, &xlm(&net.env)), 600);
    assert_eq!(
        net.transfer.balance_of(&net.transfer_addr, &xlm(&net.env)),
        400
    );
    assert!(net
        .router
        .packet_commitment(&net.client_id, &sequence_number)
        .is_none());
}

#[test]
fn cosmos_to_stellar_recv_credits_receiver_and_writes_ack() {
    let env = Env::default();
    let receiver = Address::generate(&env);
    let packet = inbound_packet_in(&env, &receiver, 500, 1);
    let net = setup_committing(env, &[Entry::Packet(packet.clone())]);
    register_voucher(&net);
    net.env.ledger().set_timestamp(1_000);

    net.router
        .recv_packet(&packet, &net.proofs[0].clone(), &HEIGHT);

    assert_eq!(
        net.transfer.balance_of(&receiver, &voucher_denom(&net)),
        500
    );
    assert!(net.router.acknowledgement(&net.client_id, &1).is_some());
    assert!(net.router.has_packet_receipt(&net.client_id, &1));
}

#[test]
fn multi_packet_recv_distinct_sequences_all_credit() {
    let env = Env::default();
    let r1 = Address::generate(&env);
    let r2 = Address::generate(&env);
    let r3 = Address::generate(&env);
    let committed_packets = [
        inbound_packet_in(&env, &r1, 100i128, 1),
        inbound_packet_in(&env, &r2, 200i128, 2),
        inbound_packet_in(&env, &r3, 300i128, 3),
    ];
    let entries: std::vec::Vec<Entry> = committed_packets
        .iter()
        .cloned()
        .map(Entry::Packet)
        .collect();
    let net = setup_committing(env, &entries);
    register_voucher(&net);
    net.env.ledger().set_timestamp(1_000);

    let mut sequence_number = 0u64;
    for (receiver, amount) in [(&r1, 100i128), (&r2, 200i128), (&r3, 300i128)] {
        sequence_number += 1;
        let packet = inbound_packet_in(&net.env, receiver, amount, sequence_number);
        net.router.recv_packet(
            &packet,
            &net.proofs[(sequence_number - 1) as usize].clone(),
            &HEIGHT,
        );
        assert!(net
            .router
            .has_packet_receipt(&net.client_id, &sequence_number));
    }

    assert_eq!(net.transfer.balance_of(&r1, &voucher_denom(&net)), 100);
    assert_eq!(net.transfer.balance_of(&r2, &voucher_denom(&net)), 200);
    assert_eq!(net.transfer.balance_of(&r3, &voucher_denom(&net)), 300);
}

#[test]
fn duplicate_recv_packet_is_rejected() {
    let env = Env::default();
    let receiver = Address::generate(&env);
    let packet = inbound_packet_in(&env, &receiver, 500, 1);
    let net = setup_committing(env, &[Entry::Packet(packet.clone())]);
    register_voucher(&net);
    net.env.ledger().set_timestamp(1_000);

    net.router
        .recv_packet(&packet, &net.proofs[0].clone(), &HEIGHT);
    let replayed = net
        .router
        .try_recv_packet(&packet, &net.proofs[0].clone(), &HEIGHT);

    assert!(replayed.is_err());
    assert_eq!(
        net.transfer.balance_of(&receiver, &voucher_denom(&net)),
        500
    );
}

#[test]
fn packet_commitment_present_after_send_absent_for_unknown() {
    let net = setup();
    let stellar_asset_contract = register_local(&net);
    let sender = Address::generate(&net.env);
    fund(&net, &stellar_asset_contract, &sender, 1_000);

    let sequence_number = net.transfer.initiate_transfer(
        &sender,
        &net.client_id,
        &xlm(&net.env),
        &250,
        &String::from_str(&net.env, "cosmos1abc"),
        &2_000,
        &String::from_str(&net.env, ""),
    );

    assert!(net
        .router
        .packet_commitment(&net.client_id, &sequence_number)
        .is_some());
    assert!(net.router.packet_commitment(&net.client_id, &999).is_none());
}

const LOCAL_CLIENT_ID: &str = "test-0";
const COUNTERPARTY_CLIENT_ID: &str = "07-tendermint-0";

fn inbound_packet_in(env: &Env, receiver: &Address, amount: i128, sequence: u64) -> Packet {
    let packet_data = FungibleTokenPacketData {
        token: Token {
            denom: xlm(env),
            amount,
        },
        sender: String::from_str(env, "cosmos1abc"),
        receiver: address_to_string(env, receiver),
        memo: String::from_str(env, ""),
    };

    Packet {
        sequence,
        source_client: String::from_str(env, COUNTERPARTY_CLIENT_ID),
        dest_client: String::from_str(env, LOCAL_CLIENT_ID),
        timeout_timestamp: 2_000,
        payloads: vec![
            env,
            Payload {
                source_port: String::from_str(env, "transfer"),
                dest_port: String::from_str(env, "transfer"),
                version: String::from_str(env, SOURCE_VERSION),
                encoding: String::from_str(env, SOURCE_ENCODING),
                value: encode_ics20_json(env, &packet_data),
            },
        ],
    }
}

fn outbound_packet_in(env: &Env, sender: &Address, amount: i128, sequence: u64) -> Packet {
    let packet_data = FungibleTokenPacketData {
        token: Token {
            denom: xlm(env),
            amount,
        },
        sender: address_to_string(env, sender),
        receiver: String::from_str(env, "cosmos1abc"),
        memo: String::from_str(env, ""),
    };

    Packet {
        sequence,
        source_client: String::from_str(env, LOCAL_CLIENT_ID),
        dest_client: String::from_str(env, COUNTERPARTY_CLIENT_ID),
        timeout_timestamp: 2_000,
        payloads: vec![
            env,
            Payload {
                source_port: String::from_str(env, "transfer"),
                dest_port: String::from_str(env, "transfer"),
                version: String::from_str(env, SOURCE_VERSION),
                encoding: String::from_str(env, SOURCE_ENCODING),
                value: encode_ics20_json(env, &packet_data),
            },
        ],
    }
}

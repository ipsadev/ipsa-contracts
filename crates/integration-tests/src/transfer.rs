#![allow(dead_code)]

use crate::tendermint::{commit_entries, register, Entry};
use soroban_sdk::{
    testutils::Address as _, token::StellarAssetClient, vec, Address, Bytes, Env, String,
};
use stellar_ibc_router::{IbcRouter, IbcRouterClient};
use stellar_ibc_transfer::{
    address_to_string, encode_ics20_json, FungibleTokenPacketData, IbcTransferApp,
    IbcTransferAppClient, Token,
};

pub const ENCODING: &str = "application/json";
pub const LOCAL_CLIENT_ID: &str = "test-0";
pub const COUNTERPARTY_CLIENT_ID: &str = "07-tendermint-0";
pub const SUCCESS_ACK_BYTE: u8 = 0x01;
pub const SUCCESS_ACK_JSON: &[u8] = b"{\"result\":\"AQ==\"}";

pub struct Fixture {
    pub proofs: std::vec::Vec<Bytes>,
    pub env: Env,
    pub router: IbcRouterClient<'static>,
    pub transfer: IbcTransferAppClient<'static>,
    pub transfer_addr: Address,
    pub source_client_id: String,
    pub counterparty_client_id: String,
    pub transfer_admin: Address,
}

pub fn setup() -> Fixture {
    setup_committing(Env::default(), &[])
}

pub fn setup_committing(env: Env, entries: &[Entry]) -> Fixture {
    env.mock_all_auths();

    let router_admin = Address::generate(&env);
    let router_addr = env.register(IbcRouter, (router_admin,));
    let router = IbcRouterClient::new(&env, &router_addr);

    let committed = commit_entries(&env, entries);
    let (_light_client, source_client_id) = register(&env, &router, "test", committed.root, 1);
    let counterparty_client_id = String::from_str(&env, "07-tendermint-0");
    router.register_counterparty(
        &source_client_id,
        &counterparty_client_id,
        &vec![&env, Bytes::from_slice(&env, b"ibc"), Bytes::new(&env)],
    );

    let transfer_admin = Address::generate(&env);
    let transfer_addr = env.register(IbcTransferApp, (router_addr, transfer_admin.clone()));
    let transfer = IbcTransferAppClient::new(&env, &transfer_addr);

    router.register_port(&String::from_str(&env, "transfer"), &transfer_addr);

    Fixture {
        env,
        router,
        transfer,
        transfer_addr,
        source_client_id,
        counterparty_client_id,
        transfer_admin,
        proofs: committed.proofs,
    }
}

pub fn xlm(env: &Env) -> String {
    String::from_str(env, "XLM")
}

pub fn register_local(f: &Fixture, denom: &String) -> Address {
    let issuer = Address::generate(&f.env);
    let stellar_asset_contract = f.env.register_stellar_asset_contract_v2(issuer).address();
    f.transfer
        .register_asset(denom, &stellar_asset_contract, &false);

    stellar_asset_contract
}

pub fn register_voucher(f: &Fixture, denom: &String) -> Address {
    let stellar_asset_contract = f
        .env
        .register_stellar_asset_contract_v2(f.transfer_addr.clone())
        .address();
    f.transfer
        .register_asset(denom, &stellar_asset_contract, &true);

    stellar_asset_contract
}

pub fn fund(f: &Fixture, stellar_asset_contract: &Address, to: &Address, amount: i128) {
    StellarAssetClient::new(&f.env, stellar_asset_contract).mint(to, &amount);
}

pub fn build_inbound_packet(
    env: &Env,
    sender_blob: &str,
    receiver: &Address,
    denom: &String,
    amount: i128,
) -> Bytes {
    let packet_data = FungibleTokenPacketData {
        token: Token {
            denom: denom.clone(),
            amount,
        },
        sender: String::from_str(env, sender_blob),
        receiver: address_to_string(env, receiver),
        memo: String::from_str(env, ""),
    };

    encode_ics20_json(env, &packet_data)
}

pub fn traced(f: &Fixture, port: &str, client: &String, denom: &str) -> String {
    let mut buf = [0u8; 128];
    let len = client.len() as usize;
    client.copy_into_slice(&mut buf[..len]);
    let client = core::str::from_utf8(&buf[..len]).unwrap();

    String::from_str(&f.env, &format!("{port}/{client}/{denom}"))
}

pub fn outbound_value(env: &Env, sender: &Address, amount: i128) -> Bytes {
    let packet = FungibleTokenPacketData {
        token: Token {
            denom: xlm(env),
            amount,
        },
        sender: address_to_string(env, sender),
        receiver: String::from_str(env, "cosmos1abc"),
        memo: String::from_str(env, ""),
    };

    encode_ics20_json(env, &packet)
}

pub fn outbound_packet(
    f: &Fixture,
    sequence: u64,
    sender: &Address,
    amount: i128,
) -> stellar_ibc_router::Packet {
    stellar_ibc_router::Packet {
        sequence,
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
                value: outbound_value(&f.env, sender, amount),
            },
        ],
    }
}

pub fn outbound_timeout(
    f: &Fixture,
    sequence: u64,
    sender: &Address,
    amount: i128,
) -> stellar_ibc_transfer::OnTimeoutPacketCallback {
    stellar_ibc_transfer::OnTimeoutPacketCallback {
        source_client: f.source_client_id.clone(),
        dest_client: f.counterparty_client_id.clone(),
        sequence,
        payload: stellar_ibc_transfer::Payload {
            source_port: String::from_str(&f.env, "transfer"),
            dest_port: String::from_str(&f.env, "transfer"),
            version: String::from_str(&f.env, "ics20-1"),
            encoding: String::from_str(&f.env, ENCODING),
            value: outbound_value(&f.env, sender, amount),
        },
    }
}

pub fn inbound_packet(
    f: &Fixture,
    version: &str,
    receiver: &Address,
    amount: i128,
) -> stellar_ibc_router::Packet {
    stellar_ibc_router::Packet {
        sequence: 1,
        source_client: f.counterparty_client_id.clone(),
        dest_client: f.source_client_id.clone(),
        timeout_timestamp: 2_000,
        payloads: vec![
            &f.env,
            stellar_ibc_router::Payload {
                source_port: String::from_str(&f.env, "transfer"),
                dest_port: String::from_str(&f.env, "transfer"),
                version: String::from_str(&f.env, version),
                encoding: String::from_str(&f.env, ENCODING),
                value: build_inbound_packet(&f.env, "cosmos1abc", receiver, &xlm(&f.env), amount),
            },
        ],
    }
}

pub fn inbound(
    env: &Env,
    sequence: u64,
    port: &str,
    encoding: &str,
    value: Bytes,
) -> stellar_ibc_router::Packet {
    stellar_ibc_router::Packet {
        sequence,
        source_client: String::from_str(env, COUNTERPARTY_CLIENT_ID),
        dest_client: String::from_str(env, LOCAL_CLIENT_ID),
        timeout_timestamp: 2_000,
        payloads: vec![
            env,
            stellar_ibc_router::Payload {
                source_port: String::from_str(env, port),
                dest_port: String::from_str(env, port),
                version: String::from_str(env, "ics20-1"),
                encoding: String::from_str(env, encoding),
                value,
            },
        ],
    }
}

pub fn inbound_packet_in(
    env: &Env,
    version: &str,
    receiver: &Address,
    amount: i128,
) -> stellar_ibc_router::Packet {
    stellar_ibc_router::Packet {
        sequence: 1,
        source_client: String::from_str(env, COUNTERPARTY_CLIENT_ID),
        dest_client: String::from_str(env, LOCAL_CLIENT_ID),
        timeout_timestamp: 2_000,
        payloads: vec![
            env,
            stellar_ibc_router::Payload {
                source_port: String::from_str(env, "transfer"),
                dest_port: String::from_str(env, "transfer"),
                version: String::from_str(env, version),
                encoding: String::from_str(env, ENCODING),
                value: build_inbound_packet(env, "cosmos1abc", receiver, &xlm(env), amount),
            },
        ],
    }
}

pub fn outbound_packet_in(
    env: &Env,
    sequence: u64,
    sender: &Address,
    amount: i128,
) -> stellar_ibc_router::Packet {
    stellar_ibc_router::Packet {
        sequence,
        source_client: String::from_str(env, LOCAL_CLIENT_ID),
        dest_client: String::from_str(env, COUNTERPARTY_CLIENT_ID),
        timeout_timestamp: 2_000,
        payloads: vec![
            env,
            stellar_ibc_router::Payload {
                source_port: String::from_str(env, "transfer"),
                dest_port: String::from_str(env, "transfer"),
                version: String::from_str(env, "ics20-1"),
                encoding: String::from_str(env, ENCODING),
                value: outbound_value(env, sender, amount),
            },
        ],
    }
}

pub fn traced_in(env: &Env, port: &str, client: &str, denom: &str) -> String {
    String::from_str(env, &format!("{port}/{client}/{denom}"))
}

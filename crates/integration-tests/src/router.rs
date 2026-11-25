#![allow(dead_code)]

use crate::tendermint::{
    commit_acknowledgement_and_absence, commit_entries, commit_packets, commit_receipt_absence,
    register, Entry,
};
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Events, Ledger},
    vec,
    xdr::{ContractEventBody, ScVal},
    Address, Bytes, Env, String,
};
use stellar_ibc_router::{
    IbcRouter, IbcRouterClient, OnAcknowledgementPacketCallback, OnRecvPacketCallback,
    OnTimeoutPacketCallback, Packet, Payload,
};

pub fn setup() -> (Env, Address, IbcRouterClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let router_id = env.register(IbcRouter, (admin.clone(),));
    let router = IbcRouterClient::new(&env, &router_id);

    let light_client = env.register(
        stellar_tendermint_light_client::TendermintLightClient,
        (router_id.clone(),),
    );

    router.register_client_type(&String::from_str(&env, "test"), &light_client);

    (env, router_id, router, light_client)
}

pub fn router_roots(env: &Env, router_id: &Address) -> std::vec::Vec<[u8; 32]> {
    let mut roots = std::vec::Vec::new();
    for event in env.events().all().filter_by_contract(router_id).events() {
        let ContractEventBody::V0(body) = &event.body;
        if body.topics.len() != 1 {
            continue;
        }
        let ScVal::Symbol(symbol) = &body.topics[0] else {
            continue;
        };
        if symbol.0.as_slice() != b"ibc_root" {
            continue;
        }
        let ScVal::Bytes(bytes) = &body.data else {
            panic!("ibc_root data must be SCVal::Bytes, got {:?}", body.data);
        };
        roots.push(
            bytes
                .as_slice()
                .try_into()
                .expect("ibc_root data must be exactly 32 bytes"),
        );
    }
    roots
}

#[contract]
pub struct EchoApp;

#[contractimpl]
impl EchoApp {
    pub fn on_recv_packet(_env: Env, cb: OnRecvPacketCallback) -> Bytes {
        cb.payload.value
    }
    pub fn on_acknowledgement_packet(_env: Env, _cb: OnAcknowledgementPacketCallback) {}
    pub fn on_timeout_packet(_env: Env, _cb: OnTimeoutPacketCallback) {}
}

pub struct PacketFlow {
    pub env: Env,
    pub router_id: Address,
    pub router: IbcRouterClient<'static>,
    pub client_id: String,
    pub counterparty_client_id: String,
    pub port_id: String,
    pub app: Address,
    pub proofs: std::vec::Vec<Bytes>,
}

pub fn setup_packet_flow() -> PacketFlow {
    setup_packet_flow_committing_in(Env::default(), &[], 1)
}

pub fn setup_packet_flow_committing_in(
    env: Env,
    packets: &[Packet],
    consensus_timestamp: u64,
) -> PacketFlow {
    env.mock_all_auths();
    env.ledger().with_mut(|li| li.timestamp = 1_000);

    let admin = Address::generate(&env);
    let router_id = env.register(IbcRouter, (admin,));
    let router = IbcRouterClient::new(&env, &router_id);

    let committed = commit_packets(&env, packets);
    let (_light_client, client_id) =
        register(&env, &router, "test", committed.root, consensus_timestamp);

    let counterparty_client_id = String::from_str(&env, "07-tendermint-0");
    let prefix = vec![&env, Bytes::from_slice(&env, b"ibc"), Bytes::new(&env)];
    router.register_counterparty(&client_id, &counterparty_client_id, &prefix);

    let app = env.register(EchoApp, ());
    let port_id = String::from_str(&env, "transfer");
    router.register_port(&port_id, &app);

    PacketFlow {
        env,
        router_id,
        router,
        client_id,
        counterparty_client_id,
        port_id,
        app,
        proofs: committed.proofs,
    }
}

pub fn make_payload(env: &Env, port: &String, value: &[u8]) -> Payload {
    Payload {
        source_port: port.clone(),
        dest_port: port.clone(),
        version: String::from_str(env, "v1"),
        encoding: String::from_str(env, "json"),
        value: Bytes::from_slice(env, value),
    }
}

pub const LOCAL_CLIENT_ID: &str = "test-0";
pub const COUNTERPARTY_CLIENT_ID: &str = "07-tendermint-0";

pub fn recv_packet_in(env: &Env, sequence: u64, value: &[u8]) -> Packet {
    Packet {
        sequence,
        source_client: String::from_str(env, COUNTERPARTY_CLIENT_ID),
        dest_client: String::from_str(env, LOCAL_CLIENT_ID),
        timeout_timestamp: 2_000,
        payloads: vec![
            env,
            make_payload(env, &String::from_str(env, "transfer"), value),
        ],
    }
}

pub fn sent_packet_in(env: &Env, sequence: u64, timeout_timestamp: u64, value: &[u8]) -> Packet {
    Packet {
        sequence,
        source_client: String::from_str(env, LOCAL_CLIENT_ID),
        dest_client: String::from_str(env, COUNTERPARTY_CLIENT_ID),
        timeout_timestamp,
        payloads: vec![
            env,
            make_payload(env, &String::from_str(env, "transfer"), value),
        ],
    }
}

pub fn flow_committing_in(env: Env, entries: &[Entry], consensus_timestamp: u64) -> PacketFlow {
    env.mock_all_auths();
    env.ledger().with_mut(|li| li.timestamp = 1_000);

    let admin = Address::generate(&env);
    let router_id = env.register(IbcRouter, (admin,));
    let router = IbcRouterClient::new(&env, &router_id);

    let committed = commit_entries(&env, entries);
    let (_light_client, client_id) =
        register(&env, &router, "test", committed.root, consensus_timestamp);

    assert_eq!(
        client_id,
        String::from_str(&env, LOCAL_CLIENT_ID),
        "the entries were committed against this client id"
    );

    let counterparty_client_id = String::from_str(&env, COUNTERPARTY_CLIENT_ID);
    let prefix = vec![&env, Bytes::from_slice(&env, b"ibc"), Bytes::new(&env)];
    router.register_counterparty(&client_id, &counterparty_client_id, &prefix);

    let app = env.register(EchoApp, ());
    let port_id = String::from_str(&env, "transfer");
    router.register_port(&port_id, &app);

    PacketFlow {
        env,
        router_id,
        router,
        client_id,
        counterparty_client_id,
        port_id,
        app,
        proofs: committed.proofs,
    }
}

pub fn timeout_flow(sequence: u64, value: &[u8]) -> (PacketFlow, Packet) {
    let env = Env::default();
    let packet = sent_packet_in(&env, sequence, 2_000, value);
    let committed = commit_receipt_absence(&env, &packet);

    env.mock_all_auths();
    env.ledger().with_mut(|li| li.timestamp = 1_000);

    let admin = Address::generate(&env);
    let router_id = env.register(IbcRouter, (admin,));
    let router = IbcRouterClient::new(&env, &router_id);

    let (_light_client, client_id) =
        register(&env, &router, "test", committed.root, ELAPSED_TIMESTAMP);

    assert_eq!(client_id, String::from_str(&env, LOCAL_CLIENT_ID));

    let counterparty_client_id = String::from_str(&env, COUNTERPARTY_CLIENT_ID);
    let prefix = vec![&env, Bytes::from_slice(&env, b"ibc"), Bytes::new(&env)];
    router.register_counterparty(&client_id, &counterparty_client_id, &prefix);

    let app = env.register(EchoApp, ());
    let port_id = String::from_str(&env, "transfer");
    router.register_port(&port_id, &app);

    (
        PacketFlow {
            env,
            router_id,
            router,
            client_id,
            counterparty_client_id,
            port_id,
            app,
            proofs: committed.proofs,
        },
        packet,
    )
}

pub fn acknowledgement_then_timeout_flow(
    sequence: u64,
    value: &[u8],
    acknowledgement: &[u8],
) -> (PacketFlow, Packet, Bytes, Bytes) {
    let env = Env::default();
    let packet = sent_packet_in(&env, sequence, 2_000, value);
    let acknowledgements = vec![&env, Bytes::from_slice(&env, acknowledgement)];
    let committed = commit_acknowledgement_and_absence(&env, &packet, &acknowledgements);

    env.mock_all_auths();
    env.ledger().with_mut(|li| li.timestamp = 1_000);

    let admin = Address::generate(&env);
    let router_id = env.register(IbcRouter, (admin,));
    let router = IbcRouterClient::new(&env, &router_id);

    let (_light_client, client_id) =
        register(&env, &router, "test", committed.root, ELAPSED_TIMESTAMP);

    assert_eq!(client_id, String::from_str(&env, LOCAL_CLIENT_ID));

    let counterparty_client_id = String::from_str(&env, COUNTERPARTY_CLIENT_ID);
    let prefix = vec![&env, Bytes::from_slice(&env, b"ibc"), Bytes::new(&env)];
    router.register_counterparty(&client_id, &counterparty_client_id, &prefix);

    let app = env.register(EchoApp, ());
    let port_id = String::from_str(&env, "transfer");
    router.register_port(&port_id, &app);

    let acknowledgement_proof = committed.proofs[0].clone();
    let absence_proof = committed.proofs[1].clone();

    (
        PacketFlow {
            env,
            router_id,
            router,
            client_id,
            counterparty_client_id,
            port_id,
            app,
            proofs: committed.proofs,
        },
        packet,
        acknowledgement_proof,
        absence_proof,
    )
}

pub fn recv_flow(packets: &[(u64, &[u8])]) -> (PacketFlow, std::vec::Vec<Packet>) {
    let env = Env::default();
    let built: std::vec::Vec<Packet> = packets
        .iter()
        .map(|(sequence, value)| recv_packet_in(&env, *sequence, value))
        .collect();
    let flow = setup_packet_flow_committing_in(env, &built, 1);

    assert_eq!(
        flow.client_id,
        String::from_str(&flow.env, LOCAL_CLIENT_ID),
        "the packets were committed against this client id"
    );

    (flow, built)
}

pub fn recv_packet_for(f: &PacketFlow, sequence: u64, value: &[u8]) -> Packet {
    let payload = make_payload(&f.env, &f.port_id, value);
    Packet {
        sequence,
        source_client: f.counterparty_client_id.clone(),
        dest_client: f.client_id.clone(),
        timeout_timestamp: 2_000,
        payloads: vec![&f.env, payload],
    }
}

pub const ELAPSED_TIMESTAMP: u64 = 1_000_000;

pub fn setup_elapsed_packet_flow() -> PacketFlow {
    setup_packet_flow_committing_in(Env::default(), &[], ELAPSED_TIMESTAMP)
}

pub fn sent_packet(f: &PacketFlow, sequence: u64, timeout_timestamp: u64, value: &[u8]) -> Packet {
    Packet {
        sequence,
        source_client: f.client_id.clone(),
        dest_client: f.counterparty_client_id.clone(),
        timeout_timestamp,
        payloads: vec![&f.env, make_payload(&f.env, &f.port_id, value)],
    }
}

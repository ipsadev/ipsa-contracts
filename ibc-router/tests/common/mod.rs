#![allow(dead_code)]

use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger},
    vec, Address, Bytes, Env, String,
};
use stellar_ibc_router::{
    IbcRouter, IbcRouterClient, OnAcknowledgementPacketCallback, OnRecvPacketCallback,
    OnTimeoutPacketCallback, Packet, Payload,
};
use stellar_mock_light_client::MockLightClient;

pub fn setup() -> (Env, Address, IbcRouterClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let router_id = env.register(IbcRouter, (admin.clone(),));
    let router = IbcRouterClient::new(&env, &router_id);

    let lc_id = env.register(MockLightClient, ());

    router.register_client_type(&String::from_str(&env, "mock"), &lc_id);

    (env, router_id, router, lc_id)
}

pub fn router_roots(env: &Env, router_id: &Address) -> std::vec::Vec<[u8; 32]> {
    use soroban_sdk::{
        testutils::Events,
        xdr::{ContractEventBody, ScVal},
    };

    let mut out = std::vec::Vec::new();
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
        out.push(
            bytes
                .as_slice()
                .try_into()
                .expect("ibc_root data must be exactly 32 bytes"),
        );
    }
    out
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
}

pub fn setup_packet_flow() -> PacketFlow {
    let (env, router_id, router, _lc_id) = setup();

    env.ledger().with_mut(|li| li.timestamp = 1_000);

    let client_id = router.create_client(
        &String::from_str(&env, "mock"),
        &Bytes::from_slice(&env, b"cs"),
        &Bytes::from_slice(&env, b"cons"),
        &1,
    );
    let counterparty_client_id = String::from_str(&env, "07-tendermint-0");
    let prefix = vec![&env, Bytes::from_slice(&env, b"ibc")];
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
    }
}

pub fn mk_payload(env: &Env, port: &String, value: &[u8]) -> Payload {
    Payload {
        source_port: port.clone(),
        dest_port: port.clone(),
        version: String::from_str(env, "v1"),
        encoding: String::from_str(env, "json"),
        value: Bytes::from_slice(env, value),
    }
}

pub fn recv_packet_for(f: &PacketFlow, sequence: u64, value: &[u8]) -> Packet {
    let payload = mk_payload(&f.env, &f.port_id, value);
    Packet {
        sequence,
        source_client: f.counterparty_client_id.clone(),
        dest_client: f.client_id.clone(),
        timeout_timestamp: 2_000,
        payloads: vec![&f.env, payload],
    }
}

#[contract]
pub struct ElapsedLightClient;

#[contractimpl]
impl ElapsedLightClient {
    pub fn initialise(
        _env: Env,
        _client_id: String,
        _client_state: Bytes,
        _consensus_state: Bytes,
        _height: u64,
    ) {
    }

    pub fn check_for_misbehaviour(_env: Env, _client_id: String, _client_message: Bytes) -> bool {
        false
    }

    pub fn update_state(_env: Env, _client_id: String, _client_message: Bytes) -> u64 {
        7
    }

    pub fn verify_membership(
        _env: Env,
        _client_id: String,
        _height: u64,
        _proof: Bytes,
        _path: Bytes,
        _value: Bytes,
    ) -> bool {
        true
    }

    pub fn verify_non_membership(
        _env: Env,
        _client_id: String,
        _height: u64,
        _proof: Bytes,
        _path: Bytes,
    ) -> bool {
        true
    }

    pub fn get_timestamp_at_height(_env: Env, _client_id: String, _height: u64) -> u64 {
        ELAPSED_TIMESTAMP
    }
}

pub const ELAPSED_TIMESTAMP: u64 = 1_000_000;

pub fn setup_elapsed_packet_flow() -> PacketFlow {
    let mut f = setup_packet_flow();
    let client_type = String::from_str(&f.env, "elapsed");
    let light_client = f.env.register(ElapsedLightClient, ());

    f.router.register_client_type(&client_type, &light_client);

    let client_id = f.router.create_client(
        &client_type,
        &Bytes::from_slice(&f.env, b"cs"),
        &Bytes::from_slice(&f.env, b"cons"),
        &1,
    );
    let prefix = vec![&f.env, Bytes::from_slice(&f.env, b"ibc")];

    f.router
        .register_counterparty(&client_id, &f.counterparty_client_id, &prefix);
    f.client_id = client_id;

    f
}

pub fn sent_packet(f: &PacketFlow, sequence: u64, timeout_timestamp: u64, value: &[u8]) -> Packet {
    Packet {
        sequence,
        source_client: f.client_id.clone(),
        dest_client: f.counterparty_client_id.clone(),
        timeout_timestamp,
        payloads: vec![&f.env, mk_payload(&f.env, &f.port_id, value)],
    }
}

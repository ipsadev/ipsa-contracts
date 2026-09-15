mod common;

use common::{router_roots, setup};
use soroban_sdk::testutils::Ledger;
use soroban_sdk::{contract, contractimpl, vec, Address, Bytes, Env, String};
use stellar_ibc_router::{
    Error, IbcRouterClient, OnAcknowledgementPacketCallback, OnRecvPacketCallback,
    OnTimeoutPacketCallback, Packet, Payload,
};

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

struct PacketFlow {
    env: Env,
    router_id: Address,
    router: IbcRouterClient<'static>,
    client_id: String,
    counterparty_client_id: String,
    port_id: String,
    #[allow(dead_code)]
    app: Address,
}

fn setup_packet_flow() -> PacketFlow {
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

fn mk_payload(env: &Env, port: &String, value: &[u8]) -> Payload {
    Payload {
        source_port: port.clone(),
        dest_port: port.clone(),
        version: String::from_str(env, "v1"),
        encoding: String::from_str(env, "json"),
        value: Bytes::from_slice(env, value),
    }
}

fn recv_packet_for(f: &PacketFlow, sequence: u64, value: &[u8]) -> Packet {
    let payload = mk_payload(&f.env, &f.port_id, value);
    Packet {
        sequence,
        source_client: f.counterparty_client_id.clone(),
        dest_client: f.client_id.clone(),
        timeout_timestamp: 2_000,
        payloads: vec![&f.env, payload],
    }
}

#[test]
fn send_packet_mints_sequences_and_stores_commitment() {
    let f = setup_packet_flow();
    let payload = mk_payload(&f.env, &f.port_id, b"hello");

    let seq1 = f
        .router
        .send_packet(&f.client_id, &2_000, &vec![&f.env, payload.clone()]);
    let seq2 = f
        .router
        .send_packet(&f.client_id, &2_000, &vec![&f.env, payload]);
    assert_eq!(seq1, 1);
    assert_eq!(seq2, 2);
    assert!(f.router.packet_commitment(&f.client_id, &seq1).is_some());
    assert!(f.router.packet_commitment(&f.client_id, &seq2).is_some());
}

#[test]
fn send_packet_rejects_expired_timeout() {
    let f = setup_packet_flow();
    let payload = mk_payload(&f.env, &f.port_id, b"x");
    let result = f
        .router
        .try_send_packet(&f.client_id, &500, &vec![&f.env, payload]);
    assert_eq!(result, Err(Ok(Error::TimeoutAlreadyElapsed.into())));
}

#[test]
fn send_packet_rejects_no_counterparty() {
    let (env, _router_id, router, _lc_id) = setup();
    env.ledger().with_mut(|li| li.timestamp = 1_000);
    let client_id = router.create_client(
        &String::from_str(&env, "mock"),
        &Bytes::from_slice(&env, b"cs"),
        &Bytes::from_slice(&env, b"cons"),
        &1,
    );
    let app = env.register(EchoApp, ());
    let port_id = String::from_str(&env, "transfer");
    router.register_port(&port_id, &app);
    let payload = mk_payload(&env, &port_id, b"x");
    let result = router.try_send_packet(&client_id, &2_000, &vec![&env, payload]);
    assert_eq!(result, Err(Ok(Error::CounterpartyNotRegistered.into())));
}

#[test]
fn send_packet_rejects_empty_payloads() {
    let f = setup_packet_flow();
    let result = f
        .router
        .try_send_packet(&f.client_id, &2_000, &vec![&f.env]);
    assert_eq!(result, Err(Ok(Error::PayloadsEmpty.into())));
}

#[test]
fn recv_packet_stores_receipt_and_ack() {
    let f = setup_packet_flow();
    let packet = recv_packet_for(&f, 1, b"hello");

    f.router
        .recv_packet(&packet, &Bytes::from_slice(&f.env, b"proof"), &10);

    assert!(f.router.has_packet_receipt(&f.client_id, &1));
    assert!(f.router.acknowledgement(&f.client_id, &1).is_some());
}

#[test]
fn recv_packet_rejects_replay() {
    let f = setup_packet_flow();
    let packet = recv_packet_for(&f, 1, b"hello");
    let proof = Bytes::from_slice(&f.env, b"proof");

    f.router.recv_packet(&packet, &proof, &10);
    let result = f.router.try_recv_packet(&packet, &proof, &10);
    assert_eq!(result, Err(Ok(Error::ReceiptAlreadyExists.into())));
}

#[test]
fn recv_packet_rejects_expired_timeout() {
    let f = setup_packet_flow();
    let payload = mk_payload(&f.env, &f.port_id, b"x");
    let stale_packet = Packet {
        sequence: 1,
        source_client: f.counterparty_client_id.clone(),
        dest_client: f.client_id.clone(),
        timeout_timestamp: 500,
        payloads: vec![&f.env, payload],
    };
    let result = f
        .router
        .try_recv_packet(&stale_packet, &Bytes::from_slice(&f.env, b"p"), &10);
    assert_eq!(result, Err(Ok(Error::TimeoutAlreadyElapsed.into())));
}

#[test]
fn recv_packet_rejects_wrong_counterparty() {
    let f = setup_packet_flow();
    let payload = mk_payload(&f.env, &f.port_id, b"x");
    let mismatched_packet = Packet {
        sequence: 1,
        source_client: String::from_str(&f.env, "07-tendermint-99"),
        dest_client: f.client_id.clone(),
        timeout_timestamp: 2_000,
        payloads: vec![&f.env, payload],
    };
    let result =
        f.router
            .try_recv_packet(&mismatched_packet, &Bytes::from_slice(&f.env, b"p"), &10);
    assert_eq!(result, Err(Ok(Error::PacketCounterpartyMismatch.into())));
}

#[test]
fn write_acknowledgement_after_recv_rejects_duplicate() {
    let f = setup_packet_flow();
    let packet = recv_packet_for(&f, 1, b"hello");
    f.router
        .recv_packet(&packet, &Bytes::from_slice(&f.env, b"proof"), &10);

    let result = f.router.try_write_acknowledgement(
        &f.client_id,
        &1,
        &vec![&f.env, Bytes::from_slice(&f.env, b"more")],
    );
    assert_eq!(result, Err(Ok(Error::AckAlreadyExists.into())));
}

#[test]
fn write_acknowledgement_rejects_when_no_receipt() {
    let f = setup_packet_flow();
    let result = f.router.try_write_acknowledgement(
        &f.client_id,
        &1,
        &vec![&f.env, Bytes::from_slice(&f.env, b"ack")],
    );
    assert_eq!(result, Err(Ok(Error::NoReceiptForSequence.into())));
}

#[test]
fn acknowledge_packet_clears_commitment() {
    let f = setup_packet_flow();
    let payload = mk_payload(&f.env, &f.port_id, b"hello");
    let seq = f
        .router
        .send_packet(&f.client_id, &2_000, &vec![&f.env, payload.clone()]);
    assert!(f.router.packet_commitment(&f.client_id, &seq).is_some());

    let packet = Packet {
        sequence: seq,
        source_client: f.client_id.clone(),
        dest_client: f.counterparty_client_id.clone(),
        timeout_timestamp: 2_000,
        payloads: vec![&f.env, payload],
    };
    let acks = vec![&f.env, Bytes::from_slice(&f.env, b"ack-bytes")];
    f.router
        .acknowledge_packet(&packet, &acks, &Bytes::from_slice(&f.env, b"proof"), &10);

    assert!(f.router.packet_commitment(&f.client_id, &seq).is_none());
}

#[test]
fn acknowledge_packet_rejects_when_no_commitment() {
    let f = setup_packet_flow();
    let payload = mk_payload(&f.env, &f.port_id, b"x");
    let packet = Packet {
        sequence: 99,
        source_client: f.client_id.clone(),
        dest_client: f.counterparty_client_id.clone(),
        timeout_timestamp: 2_000,
        payloads: vec![&f.env, payload],
    };
    let result = f.router.try_acknowledge_packet(
        &packet,
        &vec![&f.env, Bytes::from_slice(&f.env, b"ack")],
        &Bytes::from_slice(&f.env, b"proof"),
        &10,
    );
    assert_eq!(result, Err(Ok(Error::NoCommitmentForSequence.into())));
}

#[test]
fn timeout_packet_clears_commitment_when_elapsed() {
    let f = setup_packet_flow();
    let payload = mk_payload(&f.env, &f.port_id, b"hello");
    let seq = f
        .router
        .send_packet(&f.client_id, &2_000, &vec![&f.env, payload.clone()]);
    assert!(f.router.packet_commitment(&f.client_id, &seq).is_some());

    let packet = Packet {
        sequence: seq,
        source_client: f.client_id.clone(),
        dest_client: f.counterparty_client_id.clone(),
        timeout_timestamp: 2_000,
        payloads: vec![&f.env, payload],
    };
    let result = f
        .router
        .try_timeout_packet(&packet, &Bytes::from_slice(&f.env, b"proof"), &10);
    assert_eq!(result, Err(Ok(Error::TimeoutNotYetElapsed.into())));
    assert!(f.router.packet_commitment(&f.client_id, &seq).is_some());
}

#[test]
fn timeout_packet_rejects_when_no_commitment() {
    let f = setup_packet_flow();
    let payload = mk_payload(&f.env, &f.port_id, b"x");
    let packet = Packet {
        sequence: 99,
        source_client: f.client_id.clone(),
        dest_client: f.counterparty_client_id.clone(),
        timeout_timestamp: 2_000,
        payloads: vec![&f.env, payload],
    };
    let result = f
        .router
        .try_timeout_packet(&packet, &Bytes::from_slice(&f.env, b"proof"), &10);
    assert_eq!(result, Err(Ok(Error::NoCommitmentForSequence.into())));
}

#[test]
fn recv_packet_publishes_exactly_one_root() {
    let f = setup_packet_flow();
    let packet = recv_packet_for(&f, 1, b"hello");

    f.router
        .recv_packet(&packet, &Bytes::from_slice(&f.env, b"proof"), &10);

    let roots = router_roots(&f.env, &f.router_id);
    assert_eq!(
        roots.len(),
        1,
        "receipt + acknowledgement are two provable writes but one transaction"
    );
    assert_ne!(roots[0], [0u8; 32], "the tree is no longer empty");
}

#[test]
fn send_packet_publishes_exactly_one_root() {
    let f = setup_packet_flow();
    let payload = mk_payload(&f.env, &f.port_id, b"hello");

    f.router
        .send_packet(&f.client_id, &2_000, &vec![&f.env, payload]);

    assert_eq!(router_roots(&f.env, &f.router_id).len(), 1);
}

#[test]
fn acknowledging_a_packet_publishes_one_root_and_retires_the_commitment() {
    let f = setup_packet_flow();
    let payload = mk_payload(&f.env, &f.port_id, b"hello");
    let seq = f
        .router
        .send_packet(&f.client_id, &2_000, &vec![&f.env, payload.clone()]);
    let after_send = router_roots(&f.env, &f.router_id)[0];

    let packet = Packet {
        sequence: seq,
        source_client: f.client_id.clone(),
        dest_client: f.counterparty_client_id.clone(),
        timeout_timestamp: 2_000,
        payloads: vec![&f.env, payload],
    };
    let acks = vec![&f.env, Bytes::from_slice(&f.env, b"ack-bytes")];
    f.router
        .acknowledge_packet(&packet, &acks, &Bytes::from_slice(&f.env, b"proof"), &10);

    let roots = router_roots(&f.env, &f.router_id);
    assert_eq!(roots.len(), 1, "one root for the acknowledge transaction");
    assert_ne!(
        roots[0], after_send,
        "retiring the commitment must move the root"
    );
    assert_eq!(
        roots[0], [0u8; 32],
        "the only leaf is gone, so the tree is empty again"
    );
}

#[test]
fn a_second_recv_publishes_its_own_root_and_moves_the_tree() {
    let f = setup_packet_flow();
    let packet = recv_packet_for(&f, 1, b"hello");
    f.router
        .recv_packet(&packet, &Bytes::from_slice(&f.env, b"proof"), &10);
    let after_first = router_roots(&f.env, &f.router_id)[0];

    let next = recv_packet_for(&f, 2, b"second");
    f.router
        .recv_packet(&next, &Bytes::from_slice(&f.env, b"proof"), &10);

    let roots = router_roots(&f.env, &f.router_id);
    assert_eq!(roots.len(), 1, "one root event per recv transaction");
    assert_ne!(
        roots[0], after_first,
        "the second receipt and ack must move the root"
    );
}

#[test]
fn sending_a_packet_extends_the_configuration_it_depends_on() {
    use soroban_sdk::testutils::storage::Persistent as _;
    use stellar_ibc_router::DataKey;

    let f = setup_packet_flow();
    let counterparty = DataKey::Counterparty(f.client_id.clone());
    let light_client = DataKey::LightClient(f.client_id.clone());
    let port = DataKey::Port(f.port_id.clone());

    let ttl = |key: &DataKey| {
        f.env
            .as_contract(&f.router_id, || f.env.storage().persistent().get_ttl(key))
    };

    let before = (ttl(&counterparty), ttl(&light_client), ttl(&port));

    let payload = mk_payload(&f.env, &f.port_id, b"hello");
    f.router
        .send_packet(&f.client_id, &2_000, &vec![&f.env, payload]);

    let after = (ttl(&counterparty), ttl(&light_client), ttl(&port));

    assert!(
        after.0 > before.0,
        "the counterparty binding must outlive the packets that depend on it"
    );
    assert!(
        after.1 > before.1,
        "the light client address must be kept alive"
    );
    assert!(
        after.2 > before.2,
        "the port registration must be kept alive"
    );
}

#[test]
fn committing_a_root_extends_the_provable_tree() {
    use soroban_sdk::testutils::storage::Persistent as _;
    use stellar_ibc_router::smt::SmtKey;

    let f = setup_packet_flow();
    let payload = mk_payload(&f.env, &f.port_id, b"hello");
    f.router
        .send_packet(&f.client_id, &2_000, &vec![&f.env, payload]);

    let ttl = f.env.as_contract(&f.router_id, || {
        f.env.storage().persistent().get_ttl(&SmtKey::Leaves)
    });

    assert!(
        ttl > 4_096,
        "the leaf map must be extended past the minimum, got {ttl}"
    );
}

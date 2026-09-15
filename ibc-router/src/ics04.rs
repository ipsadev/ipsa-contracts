use soroban_sdk::{
    contractimpl, panic_with_error, vec, Address, Bytes, Env, IntoVal, String, Symbol, Vec,
};

use crate::errors::Error;
use crate::events::{AckPacket, RecvPacket, SendPacket, TimeoutPacket, WriteAck};
use crate::ics02::Ics02;
use crate::ics24::{commit_v2_acknowledgement, commit_v2_packet, error_ack_hash};
use crate::port::Port;
use crate::smt;
use crate::storage;
use crate::types::{
    Counterparty, DataKey, OnAcknowledgementPacketCallback, OnRecvPacketCallback,
    OnTimeoutPacketCallback, Packet, Payload,
};
use crate::{IbcRouter, IbcRouterArgs, IbcRouterClient};

pub trait Ics04 {
    fn send_packet(
        env: &Env,
        source_client_id: String,
        timeout_timestamp: u64,
        payloads: Vec<Payload>,
    ) -> u64;

    fn recv_packet(env: &Env, packet: Packet, proof: Bytes, proof_height: u64);

    fn write_acknowledgement(
        env: &Env,
        dest_client_id: String,
        sequence: u64,
        acknowledgements: Vec<Bytes>,
    );

    fn acknowledge_packet(
        env: &Env,
        packet: Packet,
        acknowledgements: Vec<Bytes>,
        proof: Bytes,
        proof_height: u64,
    );

    fn timeout_packet(env: &Env, packet: Packet, proof: Bytes, proof_height: u64);
}

#[contractimpl]
impl Ics04 for IbcRouter {
    fn send_packet(
        env: &Env,
        source_client_id: String,
        timeout_timestamp: u64,
        payloads: Vec<Payload>,
    ) -> u64 {
        if payloads.is_empty() {
            panic_with_error!(env, Error::PayloadsEmpty);
        }

        let counterparty = get_counterparty(env, &source_client_id);

        if timeout_timestamp <= env.ledger().timestamp() {
            panic_with_error!(env, Error::TimeoutAlreadyElapsed);
        }

        for payload in payloads.iter() {
            let app = get_port_app(env, &payload.source_port);
            app.require_auth();
        }

        let seq_key = DataKey::NextSeqSend(source_client_id.clone());
        let next: u64 = env
            .storage()
            .persistent()
            .get::<DataKey, u64>(&seq_key)
            .unwrap_or(0)
            + 1;
        env.storage().persistent().set(&seq_key, &next);

        let packet = Packet {
            sequence: next,
            source_client: source_client_id.clone(),
            dest_client: counterparty.client_id,
            timeout_timestamp,
            payloads,
        };

        let hash = commit_v2_packet(env, &packet);
        storage::set_packet_commitment(env, &source_client_id, next, &hash);

        smt::commit(env);

        SendPacket {
            client_id: source_client_id,
            sequence: next,
            packet,
        }
        .publish(env);

        next
    }

    fn recv_packet(env: &Env, packet: Packet, proof: Bytes, proof_height: u64) {
        let counterparty = get_counterparty(env, &packet.dest_client);
        if counterparty.client_id != packet.source_client {
            panic_with_error!(env, Error::PacketCounterpartyMismatch);
        }

        if packet.timeout_timestamp <= env.ledger().timestamp() {
            panic_with_error!(env, Error::TimeoutAlreadyElapsed);
        }

        if storage::has_packet_receipt(env, &packet.dest_client, packet.sequence) {
            panic_with_error!(env, Error::ReceiptAlreadyExists);
        }

        let expected_hash = commit_v2_packet(env, &packet);
        let cp_path = counterparty_packet_commitment_path(
            env,
            &counterparty.commitment_prefix,
            &packet.source_client,
            packet.sequence,
        );
        verify_membership(
            env,
            &packet.dest_client,
            proof_height,
            &proof,
            &cp_path,
            &expected_hash.into(),
        );

        storage::set_packet_receipt(env, &packet.dest_client, packet.sequence);

        let acks = dispatch_recv_callbacks(env, &packet);

        let ack_hash = commit_v2_acknowledgement(env, &acks);
        storage::set_ack_commitment(env, &packet.dest_client, packet.sequence, &ack_hash);

        smt::commit(env);

        RecvPacket {
            client_id: packet.dest_client.clone(),
            sequence: packet.sequence,
            packet: packet.clone(),
        }
        .publish(env);

        WriteAck {
            client_id: packet.dest_client,
            sequence: packet.sequence,
            acknowledgements: acks,
        }
        .publish(env);
    }

    fn write_acknowledgement(
        env: &Env,
        dest_client_id: String,
        sequence: u64,
        acknowledgements: Vec<Bytes>,
    ) {
        if !storage::has_packet_receipt(env, &dest_client_id, sequence) {
            panic_with_error!(env, Error::NoReceiptForSequence);
        }
        if storage::acknowledgement(env, &dest_client_id, sequence).is_some() {
            panic_with_error!(env, Error::AckAlreadyExists);
        }

        let ack_hash = commit_v2_acknowledgement(env, &acknowledgements);
        storage::set_ack_commitment(env, &dest_client_id, sequence, &ack_hash);
        smt::commit(env);

        WriteAck {
            client_id: dest_client_id,
            sequence,
            acknowledgements,
        }
        .publish(env);
    }

    fn acknowledge_packet(
        env: &Env,
        packet: Packet,
        acknowledgements: Vec<Bytes>,
        proof: Bytes,
        proof_height: u64,
    ) {
        let stored = storage::packet_commitment(env, &packet.source_client, packet.sequence)
            .unwrap_or_else(|| panic_with_error!(env, Error::NoCommitmentForSequence));
        let expected_hash = commit_v2_packet(env, &packet);
        if stored != expected_hash {
            panic_with_error!(env, Error::CommitmentMismatch);
        }

        let counterparty = get_counterparty(env, &packet.source_client);
        if counterparty.client_id != packet.dest_client {
            panic_with_error!(env, Error::PacketCounterpartyMismatch);
        }

        let ack_hash = commit_v2_acknowledgement(env, &acknowledgements);
        let cp_path = counterparty_ack_commitment_path(
            env,
            &counterparty.commitment_prefix,
            &packet.dest_client,
            packet.sequence,
        );
        verify_membership(
            env,
            &packet.source_client,
            proof_height,
            &proof,
            &cp_path,
            &ack_hash.into(),
        );

        dispatch_ack_callbacks(env, &packet, &acknowledgements);

        storage::delete_packet_commitment(env, &packet.source_client, packet.sequence);
        smt::commit(env);

        AckPacket {
            client_id: packet.source_client.clone(),
            sequence: packet.sequence,
            packet,
            acknowledgements,
        }
        .publish(env);
    }

    fn timeout_packet(env: &Env, packet: Packet, proof: Bytes, proof_height: u64) {
        let stored = storage::packet_commitment(env, &packet.source_client, packet.sequence)
            .unwrap_or_else(|| panic_with_error!(env, Error::NoCommitmentForSequence));
        let expected_hash = commit_v2_packet(env, &packet);
        if stored != expected_hash {
            panic_with_error!(env, Error::CommitmentMismatch);
        }

        let counterparty = get_counterparty(env, &packet.source_client);
        if counterparty.client_id != packet.dest_client {
            panic_with_error!(env, Error::PacketCounterpartyMismatch);
        }

        let light_client_address =
            IbcRouter::light_client_address(env, packet.source_client.clone())
                .unwrap_or_else(|| panic_with_error!(env, Error::ClientIdNotFound));

        let proof_ts: u64 = env.invoke_contract(
            &light_client_address,
            &Symbol::new(env, "get_timestamp_at_height"),
            vec![
                env,
                packet.source_client.clone().into_val(env),
                proof_height.into_val(env),
            ],
        );
        if proof_ts <= packet.timeout_timestamp {
            panic_with_error!(env, Error::TimeoutNotYetElapsed);
        }

        let cp_path = counterparty_packet_receipt_path(
            env,
            &counterparty.commitment_prefix,
            &packet.dest_client,
            packet.sequence,
        );
        verify_non_membership(env, &packet.source_client, proof_height, &proof, &cp_path);

        dispatch_timeout_callbacks(env, &packet);

        storage::delete_packet_commitment(env, &packet.source_client, packet.sequence);
        smt::commit(env);

        TimeoutPacket {
            client_id: packet.source_client.clone(),
            sequence: packet.sequence,
            packet,
        }
        .publish(env);
    }
}

fn get_counterparty(env: &Env, client_id: &String) -> Counterparty {
    let counterparty = IbcRouter::counterparty(env, client_id.clone())
        .unwrap_or_else(|| panic_with_error!(env, Error::CounterpartyNotRegistered));
    storage::extend_config(env, &DataKey::Counterparty(client_id.clone()));
    storage::extend_config(env, &DataKey::LightClient(client_id.clone()));
    counterparty
}

fn get_port_app(env: &Env, port_id: &String) -> Address {
    let app = IbcRouter::port_app(env, port_id.clone())
        .unwrap_or_else(|| panic_with_error!(env, Error::PortNotRegistered));
    storage::extend_config(env, &DataKey::Port(port_id.clone()));
    app
}

fn dispatch_recv_callbacks(env: &Env, packet: &Packet) -> Vec<Bytes> {
    let mut acks: Vec<Bytes> = Vec::new(env);
    for payload in packet.payloads.iter() {
        let app = get_port_app(env, &payload.dest_port);
        let cb = OnRecvPacketCallback {
            source_client: packet.source_client.clone(),
            dest_client: packet.dest_client.clone(),
            sequence: packet.sequence,
            payload,
        };
        let ack: Bytes = env.invoke_contract(
            &app,
            &Symbol::new(env, "on_recv_packet"),
            vec![env, cb.into_val(env)],
        );
        acks.push_back(ack);
    }
    if acks.is_empty() {
        let err_ack: Bytes = error_ack_hash(env).into();
        acks.push_back(err_ack);
    }
    acks
}

fn dispatch_ack_callbacks(env: &Env, packet: &Packet, acks: &Vec<Bytes>) {
    for (i, payload) in packet.payloads.iter().enumerate() {
        let ack = acks
            .get(i as u32)
            .unwrap_or_else(|| error_ack_hash(env).into());
        let app = get_port_app(env, &payload.source_port);
        let cb = OnAcknowledgementPacketCallback {
            source_client: packet.source_client.clone(),
            dest_client: packet.dest_client.clone(),
            sequence: packet.sequence,
            payload,
            acknowledgement: ack,
        };
        let _: () = env.invoke_contract(
            &app,
            &Symbol::new(env, "on_acknowledgement_packet"),
            vec![env, cb.into_val(env)],
        );
    }
}

fn dispatch_timeout_callbacks(env: &Env, packet: &Packet) {
    for payload in packet.payloads.iter() {
        let app = get_port_app(env, &payload.source_port);
        let cb = OnTimeoutPacketCallback {
            source_client: packet.source_client.clone(),
            dest_client: packet.dest_client.clone(),
            sequence: packet.sequence,
            payload,
        };
        let _: () = env.invoke_contract(
            &app,
            &Symbol::new(env, "on_timeout_packet"),
            vec![env, cb.into_val(env)],
        );
    }
}

fn verify_membership(
    env: &Env,
    client_id: &String,
    proof_height: u64,
    proof: &Bytes,
    path: &Bytes,
    value: &Bytes,
) {
    let light_client_address = IbcRouter::light_client_address(env, client_id.clone())
        .unwrap_or_else(|| panic_with_error!(env, Error::ClientIdNotFound));
    let ok: bool = env.invoke_contract(
        &light_client_address,
        &Symbol::new(env, "verify_membership"),
        vec![
            env,
            client_id.into_val(env),
            proof_height.into_val(env),
            proof.into_val(env),
            path.into_val(env),
            value.into_val(env),
        ],
    );
    if !ok {
        panic_with_error!(env, Error::MembershipVerificationFailed);
    }
}

fn verify_non_membership(
    env: &Env,
    client_id: &String,
    proof_height: u64,
    proof: &Bytes,
    path: &Bytes,
) {
    let light_client_address = IbcRouter::light_client_address(env, client_id.clone())
        .unwrap_or_else(|| panic_with_error!(env, Error::ClientIdNotFound));
    let ok: bool = env.invoke_contract(
        &light_client_address,
        &Symbol::new(env, "verify_non_membership"),
        vec![
            env,
            client_id.into_val(env),
            proof_height.into_val(env),
            proof.into_val(env),
            path.into_val(env),
        ],
    );
    if !ok {
        panic_with_error!(env, Error::NonMembershipVerificationFailed);
    }
}

fn counterparty_packet_commitment_path(
    env: &Env,
    prefix: &Vec<Bytes>,
    source_client_id: &String,
    sequence: u64,
) -> Bytes {
    counterparty_v2_path(env, prefix, source_client_id, 0x01, sequence)
}

fn counterparty_packet_receipt_path(
    env: &Env,
    prefix: &Vec<Bytes>,
    dest_client_id: &String,
    sequence: u64,
) -> Bytes {
    counterparty_v2_path(env, prefix, dest_client_id, 0x02, sequence)
}

fn counterparty_ack_commitment_path(
    env: &Env,
    prefix: &Vec<Bytes>,
    dest_client_id: &String,
    sequence: u64,
) -> Bytes {
    counterparty_v2_path(env, prefix, dest_client_id, 0x03, sequence)
}

fn counterparty_v2_path(
    env: &Env,
    prefix: &Vec<Bytes>,
    client_id: &String,
    discriminator: u8,
    sequence: u64,
) -> Bytes {
    let mut out = Bytes::new(env);
    for chunk in prefix.iter() {
        out.append(&chunk);
    }
    let id_len = client_id.len() as usize;
    let mut buf = [0u8; 128];
    client_id.copy_into_slice(&mut buf[..id_len]);
    buf[id_len] = discriminator;
    buf[id_len + 1..id_len + 9].copy_from_slice(&sequence.to_be_bytes());
    out.append(&Bytes::from_slice(env, &buf[..id_len + 9]));
    out
}

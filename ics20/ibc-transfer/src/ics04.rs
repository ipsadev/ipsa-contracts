use soroban_sdk::{contractimpl, panic_with_error, Bytes, Env, String};
use stellar_macros::only_admin;

use crate::{
    assets::refund,
    encoding::{decode_payload, is_success_ack, string_to_address, SUCCESS_ACK_JSON},
    errors::Error,
    events::TransferCredited,
    hooks, results,
    trace::received_denom,
    types::{
        OnAcknowledgementPacketCallback, OnRecvPacketCallback, OnTimeoutPacketCallback,
        TransferAcknowledged, TransferReceived, TransferTimedOut,
    },
    IbcTransferApp, IbcTransferAppArgs, IbcTransferAppClient, PORT, VERSION,
};

pub trait Ics04 {
    fn on_recv_packet(env: Env, callback: OnRecvPacketCallback) -> Bytes;

    fn on_acknowledgement_packet(env: Env, callback: OnAcknowledgementPacketCallback);

    fn on_timeout_packet(env: Env, callback: OnTimeoutPacketCallback);
}

#[contractimpl]
impl Ics04 for IbcTransferApp {
    #[only_admin]
    fn on_recv_packet(env: Env, callback: OnRecvPacketCallback) -> Bytes {
        let port = String::from_str(&env, PORT);

        if callback.payload.dest_port != port {
            panic_with_error!(&env, Error::UnknownDestPort);
        }

        if callback.payload.version != String::from_str(&env, VERSION) {
            panic_with_error!(&env, Error::UnsupportedVersion);
        }

        let packet = decode_payload(&env, &callback.payload.encoding, &callback.payload.value);

        if packet.token.amount <= 0 {
            panic_with_error!(&env, Error::AmountMustBePositive);
        }

        let receiver_addr = string_to_address(&env, &packet.receiver);

        let denom = received_denom(
            &env,
            &callback.payload.source_port,
            &callback.source_client,
            &callback.payload.dest_port,
            &callback.dest_client,
            &packet.token.denom,
        );

        let destination = hooks::destination_callback(&env, &packet.memo);

        if let Some(target) = &destination {
            if *target != packet.receiver {
                panic_with_error!(&env, Error::InvalidCallback);
            }
        }

        crate::voucher::credit(&env, &denom, &receiver_addr, packet.token.amount);

        TransferCredited {
            dest_client: callback.dest_client.clone(),
            sequence: callback.sequence,
            sender: packet.sender.clone(),
            receiver: receiver_addr.clone(),
            denom: denom.clone(),
            amount: packet.token.amount,
        }
        .publish(&env);

        if destination.is_some() {
            let received = TransferReceived {
                source_client: callback.source_client.clone(),
                dest_client: callback.dest_client.clone(),
                sequence: callback.sequence,
                sender: packet.sender.clone(),
                token: crate::voucher::token_address(&env, &denom),
                amount: packet.token.amount,
                memo: packet.memo.clone(),
            };

            hooks::call(&env, &receiver_addr, received);
        }

        Bytes::from_slice(&env, SUCCESS_ACK_JSON)
    }

    #[only_admin]
    fn on_acknowledgement_packet(env: Env, callback: OnAcknowledgementPacketCallback) {
        let port = String::from_str(&env, PORT);

        if callback.payload.source_port != port {
            panic_with_error!(&env, Error::UnknownSourcePort);
        }

        let packet = decode_payload(&env, &callback.payload.encoding, &callback.payload.value);
        let success = is_success_ack(&callback.acknowledgement);

        if !success {
            refund(&env, &callback.source_client, callback.sequence, &packet);
        }

        results::acknowledged(
            &env,
            &packet.sender,
            TransferAcknowledged {
                source_client: callback.source_client,
                dest_client: callback.dest_client,
                sequence: callback.sequence,
                success,
                acknowledgement: callback.acknowledgement,
            },
        );
    }

    #[only_admin]
    fn on_timeout_packet(env: Env, callback: OnTimeoutPacketCallback) {
        let port = String::from_str(&env, PORT);

        if callback.payload.source_port != port {
            panic_with_error!(&env, Error::UnknownSourcePort);
        }

        let packet = decode_payload(&env, &callback.payload.encoding, &callback.payload.value);

        refund(&env, &callback.source_client, callback.sequence, &packet);

        results::timed_out(
            &env,
            &packet.sender,
            TransferTimedOut {
                source_client: callback.source_client,
                dest_client: callback.dest_client,
                sequence: callback.sequence,
            },
        );
    }
}

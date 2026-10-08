use soroban_sdk::{contractimpl, panic_with_error, Bytes, Env, String};
use stellar_macros::only_admin;

use crate::{
    assets::refund,
    encoding::{decode_payload, is_success_ack, string_to_address, SUCCESS_ACK_JSON},
    errors::Error,
    trace::received_denom,
    types::{OnAcknowledgementPacketCallback, OnRecvPacketCallback, OnTimeoutPacketCallback},
    IbcTransferApp, IbcTransferAppArgs, IbcTransferAppClient, PORT,
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

        let packet = decode_payload(&env, &callback.payload.encoding, &callback.payload.value);
        let receiver_addr = string_to_address(&env, &packet.receiver);

        let denom = received_denom(
            &env,
            &callback.payload.source_port,
            &callback.source_client,
            &callback.payload.dest_port,
            &callback.dest_client,
            &packet.token.denom,
        );

        crate::voucher::credit(&env, &denom, &receiver_addr, packet.token.amount);

        Bytes::from_slice(&env, SUCCESS_ACK_JSON)
    }

    #[only_admin]
    fn on_acknowledgement_packet(env: Env, callback: OnAcknowledgementPacketCallback) {
        let port = String::from_str(&env, PORT);

        if callback.payload.source_port != port {
            panic_with_error!(&env, Error::UnknownSourcePort);
        }

        if is_success_ack(&callback.acknowledgement) {
            return;
        }

        let packet = decode_payload(&env, &callback.payload.encoding, &callback.payload.value);

        refund(&env, &packet);
    }

    #[only_admin]
    fn on_timeout_packet(env: Env, callback: OnTimeoutPacketCallback) {
        let port = String::from_str(&env, PORT);

        if callback.payload.source_port != port {
            panic_with_error!(&env, Error::UnknownSourcePort);
        }

        let packet = decode_payload(&env, &callback.payload.encoding, &callback.payload.value);

        refund(&env, &packet);
    }
}

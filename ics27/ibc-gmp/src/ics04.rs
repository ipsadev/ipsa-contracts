use soroban_sdk::{contractimpl, panic_with_error, xdr::ToXdr, Bytes, Env, IntoVal, String};
use stellar_macros::only_admin;

use crate::{
    account,
    encoding::{decode_acknowledgement, decode_call, decode_packet, encode_acknowledgement},
    errors::Error,
    limits, results, storage,
    types::{
        GmpAcknowledged, GmpTimedOut, OnAcknowledgementPacketCallback, OnRecvPacketCallback,
        OnTimeoutPacketCallback,
    },
    IbcGmpApp, IbcGmpAppArgs, IbcGmpAppClient, PORT, VERSION,
};

pub trait Ics04 {
    fn on_recv_packet(env: Env, callback: OnRecvPacketCallback) -> Bytes;

    fn on_acknowledgement_packet(env: Env, callback: OnAcknowledgementPacketCallback);

    fn on_timeout_packet(env: Env, callback: OnTimeoutPacketCallback);
}

#[contractimpl]
impl Ics04 for IbcGmpApp {
    #[only_admin]
    fn on_recv_packet(env: Env, callback: OnRecvPacketCallback) -> Bytes {
        if callback.payload.dest_port != String::from_str(&env, PORT) {
            panic_with_error!(&env, Error::UnknownDestPort);
        }

        if callback.payload.version != String::from_str(&env, VERSION) {
            panic_with_error!(&env, Error::UnsupportedVersion);
        }

        let packet = decode_packet(&env, &callback.payload.encoding, &callback.payload.value);
        limits::check(&env, &packet);

        let call = decode_call(&env, &packet.payload);

        if !packet.receiver.is_empty() && packet.receiver != call.target.to_string() {
            panic_with_error!(&env, Error::ReceiverMismatch);
        }

        if let Some(router) = storage::router(&env) {
            if call.target == router {
                panic_with_error!(&env, Error::TargetIsRouter);
            }
        }

        let account = account::ensure(&env, &callback.dest_client, &packet.sender, &packet.salt);

        let result: soroban_sdk::Val = env.invoke_contract(
            &account,
            &soroban_sdk::symbol_short!("execute"),
            soroban_sdk::vec![
                &env,
                call.target.into_val(&env),
                call.function.into_val(&env),
                call.args.into_val(&env),
            ],
        );

        encode_acknowledgement(&env, &callback.payload.encoding, result.to_xdr(&env))
    }

    #[only_admin]
    fn on_acknowledgement_packet(env: Env, callback: OnAcknowledgementPacketCallback) {
        if callback.payload.source_port != String::from_str(&env, PORT) {
            panic_with_error!(&env, Error::UnknownSourcePort);
        }

        let packet = decode_packet(&env, &callback.payload.encoding, &callback.payload.value);

        let success = !results::is_error_acknowledgement(&env, &callback.acknowledgement);
        let decoded = if success {
            decode_acknowledgement(&env, &callback.payload.encoding, &callback.acknowledgement)
        } else {
            None
        };

        let result = GmpAcknowledged {
            source_client: callback.source_client,
            dest_client: callback.dest_client,
            sequence: callback.sequence,
            success,
            result: decoded.unwrap_or_else(|| Bytes::new(&env)),
            acknowledgement: callback.acknowledgement,
        };

        results::acknowledged(&env, &packet.sender, result);
    }

    #[only_admin]
    fn on_timeout_packet(env: Env, callback: OnTimeoutPacketCallback) {
        if callback.payload.source_port != String::from_str(&env, PORT) {
            panic_with_error!(&env, Error::UnknownSourcePort);
        }

        let packet = decode_packet(&env, &callback.payload.encoding, &callback.payload.value);

        let result = GmpTimedOut {
            source_client: callback.source_client,
            dest_client: callback.dest_client,
            sequence: callback.sequence,
        };

        results::timed_out(&env, &packet.sender, result);
    }
}

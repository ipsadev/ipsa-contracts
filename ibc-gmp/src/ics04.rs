use soroban_sdk::{contractimpl, panic_with_error, xdr::ToXdr, Bytes, Env, IntoVal, String};
use stellar_macros::only_admin;

use crate::{
    account,
    encoding::{decode_call, decode_packet},
    errors::Error,
    limits, storage,
    types::{
        Acknowledgement, OnAcknowledgementPacketCallback, OnRecvPacketCallback,
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

        Acknowledgement {
            result: result.to_xdr(&env),
        }
        .to_xdr(&env)
    }

    #[only_admin]
    fn on_acknowledgement_packet(env: Env, callback: OnAcknowledgementPacketCallback) {
        if callback.payload.source_port != String::from_str(&env, PORT) {
            panic_with_error!(&env, Error::UnknownSourcePort);
        }
    }

    #[only_admin]
    fn on_timeout_packet(env: Env, callback: OnTimeoutPacketCallback) {
        if callback.payload.source_port != String::from_str(&env, PORT) {
            panic_with_error!(&env, Error::UnknownSourcePort);
        }
    }
}

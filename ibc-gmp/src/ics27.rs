use soroban_sdk::{
    contractimpl, panic_with_error, vec, Address, Bytes, Env, IntoVal, String, Symbol, Vec,
};

use crate::errors::Error;
use crate::limits;
use crate::storage;
use crate::types::{GmpPacketData, Payload};
use crate::{IbcGmpApp, IbcGmpAppArgs, IbcGmpAppClient, PORT, VERSION};

pub trait Ics27 {
    fn send_call(
        env: Env,
        sender: Address,
        source_client_id: String,
        receiver: String,
        salt: Bytes,
        payload: Bytes,
        timeout_timestamp: u64,
        memo: String,
        encoding: String,
    ) -> u64;
}

#[contractimpl]
impl Ics27 for IbcGmpApp {
    fn send_call(
        env: Env,
        sender: Address,
        source_client_id: String,
        receiver: String,
        salt: Bytes,
        payload: Bytes,
        timeout_timestamp: u64,
        memo: String,
        encoding: String,
    ) -> u64 {
        sender.require_auth();

        if source_client_id.is_empty() {
            panic_with_error!(&env, Error::InvalidPacketData);
        }

        let packet = GmpPacketData {
            sender: sender.to_string(),
            receiver,
            salt,
            payload,
            memo,
        };

        limits::check(&env, &packet);

        let port = String::from_str(&env, PORT);
        let payloads: Vec<Payload> = vec![
            &env,
            Payload {
                source_port: port.clone(),
                dest_port: port,
                version: String::from_str(&env, VERSION),
                value: crate::encoding::encode_packet(&env, &encoding, &packet),
                encoding,
            },
        ];

        let router =
            storage::router(&env).unwrap_or_else(|| panic_with_error!(&env, Error::RouterNotSet));

        env.invoke_contract::<u64>(
            &router,
            &Symbol::new(&env, "send_packet"),
            vec![
                &env,
                source_client_id.into_val(&env),
                timeout_timestamp.into_val(&env),
                payloads.into_val(&env),
            ],
        )
    }
}

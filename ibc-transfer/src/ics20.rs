use soroban_sdk::{
    contractimpl, panic_with_error, vec, Address, BytesN, Env, IntoVal, String, Symbol, Vec,
};
use stellar_access::{access_control::set_admin, ownable::set_owner};
use stellar_macros::only_owner;

use crate::encoding::{address_to_string, encode_ics20_json};
use crate::errors::Error;
use crate::limits::enforce_rate_limit;
use crate::storage::DataKey;
use crate::types::{FungibleTokenPacketData, Payload, Token};
use crate::{IbcTransferApp, IbcTransferAppArgs, IbcTransferAppClient, ENCODING, PORT, VERSION};

#[contractimpl]
impl IbcTransferApp {
    pub fn __constructor(env: Env, router: Address, owner: Address) {
        env.storage().instance().set(&DataKey::Router, &router);
        set_owner(&env, &owner);
        set_admin(&env, &router);
    }

    #[only_owner]
    pub fn set_voucher_wasm(env: Env, wasm_hash: BytesN<32>) {
        crate::voucher::set_voucher_wasm(&env, wasm_hash);
    }
}

pub trait Ics20 {
    fn initiate_transfer(
        env: Env,
        sender: Address,
        source_client_id: String,
        denom: String,
        amount: i128,
        receiver: String,
        timeout_timestamp: u64,
        memo: String,
    ) -> u64;
}

#[contractimpl]
impl Ics20 for IbcTransferApp {
    fn initiate_transfer(
        env: Env,
        sender: Address,
        source_client_id: String,
        denom: String,
        amount: i128,
        receiver: String,
        timeout_timestamp: u64,
        memo: String,
    ) -> u64 {
        sender.require_auth();

        if amount <= 0 {
            panic_with_error!(&env, Error::AmountMustBePositive);
        }

        enforce_rate_limit(&env, &denom, amount);

        crate::voucher::debit(&env, &denom, &sender, amount);

        let packet = FungibleTokenPacketData {
            token: Token {
                denom: denom.clone(),
                amount,
            },
            sender: address_to_string(&env, &sender),
            receiver,
            memo,
        };
        let value = encode_ics20_json(&env, &packet);

        let port = String::from_str(&env, PORT);
        let payload = Payload {
            source_port: port.clone(),
            dest_port: port,
            version: String::from_str(&env, VERSION),
            encoding: String::from_str(&env, ENCODING),
            value,
        };

        let router_addr: Address = env
            .storage()
            .instance()
            .get(&DataKey::Router)
            .unwrap_or_else(|| panic_with_error!(&env, Error::RouterNotSet));
        let payloads: Vec<Payload> = vec![&env, payload];

        env.invoke_contract::<u64>(
            &router_addr,
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

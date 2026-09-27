use soroban_sdk::{
    contractimpl, panic_with_error, vec, Address, BytesN, Env, IntoVal, String, Symbol, Vec,
};
use stellar_access::{access_control::set_admin, ownable::set_owner};
use stellar_macros::only_owner;

use crate::encoding::{
    address_to_string, encode_payload, require_supported_encoding, ENCODING_JSON,
};
use crate::errors::Error;
use crate::limits::enforce_rate_limit;
use crate::storage::DataKey;
use crate::types::{FungibleTokenPacketData, Payload, Token};
use crate::{IbcTransferApp, IbcTransferAppArgs, IbcTransferAppClient, PORT, VERSION};

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

    #[only_owner]
    pub fn upgrade(env: Env, wasm_hash: BytesN<32>) {
        env.deployer().update_current_contract_wasm(wasm_hash);
    }

    #[only_owner]
    pub fn set_client_encoding(env: Env, client_id: String, encoding: String) {
        require_supported_encoding(&env, &encoding);

        env.storage()
            .persistent()
            .set(&DataKey::ClientEncoding(client_id), &encoding);
    }

    pub fn client_encoding(env: Env, client_id: String) -> String {
        env.storage()
            .persistent()
            .get(&DataKey::ClientEncoding(client_id))
            .unwrap_or_else(|| String::from_str(&env, ENCODING_JSON))
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
        let encoding = Self::client_encoding(env.clone(), source_client_id.clone());
        let value = encode_payload(&env, &encoding, &packet);

        let port = String::from_str(&env, PORT);
        let payload = Payload {
            source_port: port.clone(),
            dest_port: port,
            version: String::from_str(&env, VERSION),
            encoding,
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

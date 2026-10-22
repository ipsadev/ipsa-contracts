#![no_std]

use soroban_sdk::{contract, contractimpl, Address, Bytes, BytesN, Env, String};
use stellar_access::{access_control::set_admin, ownable::Ownable};
use stellar_macros::only_owner;

mod abi;
mod account;
mod encoding;
mod errors;
mod ics04;
mod ics27;
mod json;
mod limits;
mod protobuf;
mod results;
mod storage;
mod types;

pub use crate::{
    errors::Error,
    ics04::Ics04,
    ics27::Ics27,
    types::{
        Acknowledgement, DerivedAccount, GmpAcknowledged, GmpCall, GmpPacketData, GmpTimedOut,
        OnAcknowledgementPacketCallback, OnRecvPacketCallback, OnTimeoutPacketCallback, Payload,
    },
};

pub(crate) const PORT: &str = "gmpport";
pub(crate) const VERSION: &str = "ics27-2";
pub(crate) const ENCODING: &str = "application/x-soroban-xdr";
pub(crate) const ENCODING_JSON: &str = "application/json";
pub(crate) const ENCODING_SOLIDITY_ABI: &str = "application/x-solidity-abi";
pub(crate) const ENCODING_PROTOBUF: &str = "application/x-protobuf";

#[contract]
pub struct IbcGmpApp;

#[contractimpl(contracttrait)]
impl Ownable for IbcGmpApp {}

#[contractimpl]
impl IbcGmpApp {
    pub fn __constructor(env: Env, owner: Address, router: Address) {
        stellar_access::ownable::set_owner(&env, &owner);
        set_admin(&env, &router);
        storage::set_router(&env, &router);
    }

    #[only_owner]
    pub fn set_account_wasm(env: &Env, wasm_hash: BytesN<32>) {
        storage::set_account_wasm(env, &wasm_hash);
    }

    #[only_owner]
    pub fn upgrade(env: &Env, wasm_hash: BytesN<32>) {
        env.deployer().update_current_contract_wasm(wasm_hash);
    }

    pub fn port(env: &Env) -> String {
        String::from_str(env, PORT)
    }

    pub fn version(env: &Env) -> String {
        String::from_str(env, VERSION)
    }

    pub fn encoding(env: &Env) -> String {
        String::from_str(env, ENCODING)
    }

    pub fn encode_packet(env: &Env, encoding: String, packet: GmpPacketData) -> Bytes {
        crate::encoding::encode_packet(env, &encoding, &packet)
    }

    pub fn decode_packet(env: &Env, encoding: String, value: Bytes) -> GmpPacketData {
        crate::encoding::decode_packet(env, &encoding, &value)
    }

    pub fn account_address(
        env: &Env,
        client_id: String,
        sender: String,
        salt: Bytes,
    ) -> DerivedAccount {
        account::derived(env, &client_id, &sender, &salt)
    }
}

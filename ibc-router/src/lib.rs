#![no_std]

use soroban_sdk::{contract, contractimpl, Address, BytesN, Env};
use stellar_access::ownable::{set_owner, Ownable};
use stellar_macros::only_owner;

mod errors;
mod events;
mod ics02;
mod ics04;
pub mod ics24;
pub mod identifiers;
mod port;
pub mod smt;
mod storage;
mod types;

pub use errors::Error;
pub use events::{
    AckPacket, ClientCreated, ClientFrozen, ClientMigrated, ClientTypeRegistered, ClientUpdated,
    CounterpartyRegistered, PortReassigned, PortRegistered, PortRemoved, RecvPacket, SendPacket,
    TimeoutPacket, WriteAck,
};
pub use types::{
    Counterparty, DataKey, Height, OnAcknowledgementPacketCallback, OnRecvPacketCallback,
    OnTimeoutPacketCallback, Packet, Payload,
};

#[contract]
pub struct IbcRouter;

#[contractimpl]
impl IbcRouter {
    pub fn __constructor(env: Env, owner: Address) {
        set_owner(&env, &owner);
    }

    pub fn commit_root(env: Env) -> BytesN<32> {
        smt::commit(&env)
    }

    pub fn ibc_root(env: Env) -> BytesN<32> {
        smt::stored_root(&env)
    }

    #[only_owner]
    pub fn upgrade(env: Env, wasm_hash: BytesN<32>) {
        env.deployer().update_current_contract_wasm(wasm_hash);
    }
}

#[contractimpl(contracttrait)]
impl Ownable for IbcRouter {}

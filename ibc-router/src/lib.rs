#![no_std]

use soroban_sdk::{contract, contractimpl, Address, BytesN, Env};
use stellar_access::ownable::{set_owner, Ownable};

mod errors;
mod events;
mod ics02;
mod ics04;
mod ics24;
mod identifiers;
mod port;
pub mod smt;
mod storage;
mod types;

pub use errors::Error;
pub use events::{AckPacket, RecvPacket, SendPacket, TimeoutPacket, WriteAck};
pub use types::{OnAcknowledgementPacketCallback, OnRecvPacketCallback, OnTimeoutPacketCallback};

pub use types::{Counterparty, DataKey, Height, Packet, Payload};

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
}

#[contractimpl(contracttrait)]
impl Ownable for IbcRouter {}

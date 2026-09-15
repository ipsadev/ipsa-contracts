#![no_std]

use soroban_sdk::{contract, contractimpl, Address};
use stellar_access::ownable::Ownable;

mod assets;
mod encoding;
mod errors;
mod ics04;
mod ics20;
mod limits;
mod storage;
mod types;
mod voucher;

pub use crate::encoding::{address_to_string, decode_packet, encode_ics20_json};
pub use crate::errors::Error;
pub use crate::types::{
    AssetInfo, AssetKind, FungibleTokenPacketData, OnAcknowledgementPacketCallback,
    OnRecvPacketCallback, OnTimeoutPacketCallback, Payload, Token,
};

pub(crate) const PORT: &str = "transfer";
pub(crate) const VERSION: &str = "ics20-1";
pub(crate) const ENCODING: &str = "application/json";

#[contract]
pub struct IbcTransferApp;

#[contractimpl(contracttrait)]
impl Ownable for IbcTransferApp {}

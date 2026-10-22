#![no_std]

use soroban_sdk::{contract, contractimpl, Address};
use stellar_access::ownable::Ownable;

mod assets;
mod encoding;
mod errors;
mod events;
mod hooks;
mod ics04;
mod ics20;
mod limits;
mod results;
mod storage;
mod trace;
mod types;
mod voucher;

pub use crate::{
    encoding::{
        address_to_string, decode_packet, decode_packet_abi, decode_payload, encode_ics20_abi,
        encode_ics20_json, encode_payload, ENCODING_JSON, ENCODING_SOLIDITY_ABI,
    },
    errors::Error,
    events::{TransferCredited, TransferRefunded, TransferSent},
    trace::received_denom,
    types::{
        AssetInfo, AssetKind, FungibleTokenPacketData, OnAcknowledgementPacketCallback,
        OnRecvPacketCallback, OnTimeoutPacketCallback, Payload, Token, TransferAcknowledged,
        TransferReceived, TransferTimedOut,
    },
};

pub(crate) const PORT: &str = "transfer";
pub(crate) const VERSION: &str = "ics20-1";

#[contract]
pub struct IbcTransferApp;

#[contractimpl(contracttrait)]
impl Ownable for IbcTransferApp {}

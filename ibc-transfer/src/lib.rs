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

pub(crate) use ipsa::ics20::{PORT, VERSION};

pub use crate::{
    assets::Assets,
    encoding::{
        address_to_string, decode_packet, decode_packet_abi, decode_payload, encode_ics20_abi,
        encode_ics20_json, encode_payload, ENCODING_JSON, ENCODING_SOLIDITY_ABI,
    },
    errors::Error,
    events::{TransferCredited, TransferRefunded, TransferSent},
    ics04::Ics04,
    ics20::Ics20,
    limits::Limits,
    trace::received_denom,
    types::{
        AssetInfo, AssetKind, FungibleTokenPacketData, OnAcknowledgementPacketCallback,
        OnRecvPacketCallback, OnTimeoutPacketCallback, Payload, Token, TransferAcknowledged,
        TransferReceived, TransferTimedOut,
    },
};

#[contract]
pub struct IbcTransferApp;

#[contractimpl(contracttrait)]
impl Ownable for IbcTransferApp {}

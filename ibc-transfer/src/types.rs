use soroban_sdk::{contracttype, Address, Bytes, String};

#[contracttype]
#[derive(Clone)]
pub struct Payload {
    pub source_port: String,
    pub dest_port: String,
    pub version: String,
    pub encoding: String,
    pub value: Bytes,
}

#[contracttype]
#[derive(Clone)]
pub struct OnRecvPacketCallback {
    pub source_client: String,
    pub dest_client: String,
    pub sequence: u64,
    pub payload: Payload,
}

#[contracttype]
#[derive(Clone)]
pub struct OnAcknowledgementPacketCallback {
    pub source_client: String,
    pub dest_client: String,
    pub sequence: u64,
    pub payload: Payload,
    pub acknowledgement: Bytes,
}

#[contracttype]
#[derive(Clone)]
pub struct OnTimeoutPacketCallback {
    pub source_client: String,
    pub dest_client: String,
    pub sequence: u64,
    pub payload: Payload,
}

#[contracttype]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AssetKind {
    Local,
    Voucher,
}

#[contracttype]
#[derive(Clone)]
pub struct AssetInfo {
    pub address: Address,
    pub kind: AssetKind,
}

#[contracttype]
#[derive(Clone)]
pub struct AssetEntry {
    pub denom: String,
    pub address: Address,
    pub kind: AssetKind,
}

#[contracttype]
#[derive(Clone)]
pub struct Token {
    pub denom: String,
    pub amount: i128,
}

#[contracttype]
#[derive(Clone)]
pub struct FungibleTokenPacketData {
    pub token: Token,
    pub sender: String,
    pub receiver: String,
    pub memo: String,
}

#[contracttype]
#[derive(Clone)]
pub struct TransferReceived {
    pub source_client: String,
    pub dest_client: String,
    pub sequence: u64,
    pub sender: String,
    pub token: Address,
    pub amount: i128,
    pub memo: String,
}

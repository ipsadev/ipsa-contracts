use soroban_sdk::{contracttype, Address, Bytes, String, Symbol, Val, Vec};

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
#[derive(Clone)]
pub struct GmpPacketData {
    pub sender: String,
    pub receiver: String,
    pub salt: Bytes,
    pub payload: Bytes,
    pub memo: String,
}

#[contracttype]
#[derive(Clone)]
pub struct GmpCall {
    pub target: Address,
    pub function: Symbol,
    pub args: Vec<Val>,
}

#[contracttype]
#[derive(Clone)]
pub struct Acknowledgement {
    pub result: Bytes,
}

#[contracttype]
#[derive(Clone)]
pub struct DerivedAccount {
    pub address: Address,
    pub materialized: bool,
}

pub use ipsa::types::{
    Counterparty, OnAcknowledgementPacketCallback, OnRecvPacketCallback, OnTimeoutPacketCallback,
    Packet, Payload,
};
use soroban_sdk::{contracttype, String};

#[contracttype]
#[derive(Clone)]
pub struct Height {
    pub revision_number: u64,
    pub revision_height: u64,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    NextClientId(String),
    ClientTypeAddr(String),
    ClientType(String),
    LightClient(String),
    Counterparty(String),
    Frozen(String),
    Port(String),
    NextSeqSend(String),
    PortPacketsInFlight(String),
}

use soroban_sdk::{contracttype, Bytes, String, Vec};

#[contracttype]
#[derive(Clone)]
pub struct Counterparty {
    pub client_id: String,
    pub commitment_prefix: Vec<Bytes>,
}

#[contracttype]
#[derive(Clone)]
pub struct Height {
    pub revision_number: u64,
    pub revision_height: u64,
}

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
pub struct Packet {
    pub sequence: u64,
    pub source_client: String,
    pub dest_client: String,
    pub timeout_timestamp: u64,
    pub payloads: Vec<Payload>,
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

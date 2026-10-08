use soroban_sdk::{contractevent, Address, Bytes, BytesN, String, Vec};

use crate::types::Packet;

#[contractevent]
#[derive(Clone)]
pub struct SendPacket {
    #[topic]
    pub client_id: String,
    #[topic]
    pub sequence: u64,
    pub packet: Packet,
}

#[contractevent]
#[derive(Clone)]
pub struct RecvPacket {
    #[topic]
    pub client_id: String,
    #[topic]
    pub sequence: u64,
    pub packet: Packet,
}

#[contractevent]
#[derive(Clone)]
pub struct WriteAck {
    #[topic]
    pub client_id: String,
    #[topic]
    pub sequence: u64,
    pub acknowledgements: Vec<Bytes>,
}

#[contractevent]
#[derive(Clone)]
pub struct AckPacket {
    #[topic]
    pub client_id: String,
    #[topic]
    pub sequence: u64,
    pub packet: Packet,
    pub acknowledgements: Vec<Bytes>,
}

#[contractevent]
#[derive(Clone)]
pub struct TimeoutPacket {
    #[topic]
    pub client_id: String,
    #[topic]
    pub sequence: u64,
    pub packet: Packet,
}

#[contractevent(topics = ["ibc_root"], data_format = "single-value")]
#[derive(Clone)]
pub struct IbcRoot {
    pub root: BytesN<32>,
}

#[contractevent]
#[derive(Clone)]
pub struct ClientMigrated {
    #[topic]
    pub client_id: String,
    pub light_client: Address,
}

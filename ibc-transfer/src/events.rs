use soroban_sdk::{contractevent, Address, String};

#[contractevent]
#[derive(Clone)]
pub struct TransferSent {
    #[topic]
    pub source_client: String,
    #[topic]
    pub sequence: u64,
    pub sender: Address,
    pub denom: String,
    pub amount: i128,
    pub receiver: String,
}

#[contractevent]
#[derive(Clone)]
pub struct TransferCredited {
    #[topic]
    pub dest_client: String,
    #[topic]
    pub sequence: u64,
    pub sender: String,
    pub receiver: Address,
    pub denom: String,
    pub amount: i128,
}

#[contractevent]
#[derive(Clone)]
pub struct TransferRefunded {
    #[topic]
    pub source_client: String,
    #[topic]
    pub sequence: u64,
    pub sender: Address,
    pub denom: String,
    pub amount: i128,
}

use soroban_sdk::{contractevent, Address, BytesN, String};

#[contractevent]
#[derive(Clone)]
pub struct OrderFilled {
    #[topic]
    pub key: BytesN<32>,
    pub source_client: String,
    pub sequence: u64,
    pub provider: Address,
    pub recipient: Address,
    pub token: Address,
    pub paid: i128,
}

#[contractevent]
#[derive(Clone)]
pub struct OrderSettled {
    #[topic]
    pub key: BytesN<32>,
    pub paid_to: Address,
    pub token: Address,
    pub amount: i128,
    pub filled: bool,
}

#[contractevent]
#[derive(Clone)]
pub struct RefundNotified {
    #[topic]
    pub key: BytesN<32>,
    pub provider_evm: String,
    pub gmp_sequence: u64,
}

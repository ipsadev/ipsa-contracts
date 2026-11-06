use soroban_sdk::{contracttype, Address, String};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Order {
    pub source_client: String,
    pub dest_client: String,
    pub sequence: u64,
    pub sender: String,
    pub denom: String,
    pub amount: i128,
    pub memo: String,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Fill {
    pub provider: Address,
    pub provider_evm: String,
    pub recipient: Address,
    pub token: Address,
    pub dest_client: String,
    pub sender: String,
    pub filled_at: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    pub transfer_app: Address,
    pub gmp_app: Address,
    pub notice_encoding: String,
    pub notice_timeout_seconds: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Terms {
    pub recipient: Address,
    pub fee: i128,
    pub deadline: u64,
}

use soroban_sdk::{contracttype, String};

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Router,
    Asset(String),
    AssetDenoms,
    DailyCap(String),
    Usage(String, u64),
    VoucherWasmHash,
    Voucher(String),
}

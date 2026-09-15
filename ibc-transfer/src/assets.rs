use crate::{
    encoding::string_to_address,
    errors::Error,
    storage::DataKey,
    types::{AssetEntry, AssetInfo, AssetKind, FungibleTokenPacketData},
    IbcTransferApp, IbcTransferAppArgs, IbcTransferAppClient,
};
use soroban_sdk::{contractimpl, panic_with_error, token, Address, Env, String, Vec};
use stellar_macros::only_owner;

pub trait Assets {
    fn register_asset(env: Env, denom: String, address: Address, voucher: bool);

    fn unregister_asset(env: Env, denom: String);

    fn asset(env: Env, denom: String);

    fn assets(env: Env) -> Vec<AssetEntry>;

    fn mint(env: Env, to: Address, denom: String, amount: i128) -> Result<(), Error>;

    fn balance_of(env: Env, who: Address, denom: String) -> i128;
}

#[contractimpl]
impl Assets for IbcTransferApp {
    #[only_owner]
    fn register_asset(env: Env, denom: String, address: Address, voucher: bool) {
        let kind = if voucher {
            AssetKind::Voucher
        } else {
            AssetKind::Local
        };

        index_add(&env, &denom);

        env.storage()
            .persistent()
            .set(&DataKey::Asset(denom), &AssetInfo { address, kind });
    }

    #[only_owner]
    fn unregister_asset(env: Env, denom: String) {
        index_remove(&env, &denom);

        env.storage().persistent().remove(&DataKey::Asset(denom));
    }

    fn asset(env: Env, denom: String) {
        details(&env, &denom);
    }

    fn assets(env: Env) -> Vec<AssetEntry> {
        let mut out = Vec::new(&env);

        for denom in denoms(&env).iter() {
            if let Some(info) = try_details(&env, &denom) {
                out.push_back(AssetEntry {
                    denom,
                    address: info.address,
                    kind: info.kind,
                });
            }
        }

        out
    }

    #[only_owner]
    fn mint(env: Env, to: Address, denom: String, amount: i128) -> Result<(), Error> {
        if amount <= 0 {
            panic_with_error!(&env, Error::AmountMustBePositive);
        }

        let info = details(&env, &denom);

        token::StellarAssetClient::new(&env, &info.address).mint(&to, &amount);

        Ok(())
    }

    fn balance_of(env: Env, who: Address, denom: String) -> i128 {
        crate::voucher::balance(&env, &who, &denom)
    }
}

pub(crate) const MAX_ASSETS: u32 = 256;

pub(crate) fn denoms(env: &Env) -> Vec<String> {
    env.storage()
        .persistent()
        .get::<_, Vec<String>>(&DataKey::AssetDenoms)
        .unwrap_or_else(|| Vec::new(env))
}

fn index_add(env: &Env, denom: &String) {
    let mut list = denoms(env);

    if list.iter().any(|existing| existing == *denom) {
        return;
    }

    if list.len() >= MAX_ASSETS {
        panic_with_error!(env, Error::TooManyAssets);
    }

    list.push_back(denom.clone());

    env.storage().persistent().set(&DataKey::AssetDenoms, &list);
}

fn index_remove(env: &Env, denom: &String) {
    let list = denoms(env);
    let mut out = Vec::new(env);

    for existing in list.iter() {
        if existing != *denom {
            out.push_back(existing);
        }
    }

    env.storage().persistent().set(&DataKey::AssetDenoms, &out);
}

pub(crate) fn details(env: &Env, denom: &String) -> AssetInfo {
    try_details(env, denom).unwrap_or_else(|| panic_with_error!(env, Error::UnknownAsset))
}

pub(crate) fn try_details(env: &Env, denom: &String) -> Option<AssetInfo> {
    env.storage()
        .persistent()
        .get::<_, AssetInfo>(&DataKey::Asset(denom.clone()))
}

pub(crate) fn refund(env: &Env, packet: &FungibleTokenPacketData) {
    let sender_addr = string_to_address(env, &packet.sender);

    crate::voucher::credit(env, &packet.token.denom, &sender_addr, packet.token.amount);
}

use soroban_sdk::{
    panic_with_error, token, vec, Address, Bytes, BytesN, Env, IntoVal, String, Symbol,
};

use crate::{assets::try_details, errors::Error, storage::DataKey, types::AssetKind};

const MAX_DENOM_BYTES: usize = 256;

pub(crate) fn set_voucher_wasm(env: &Env, wasm_hash: BytesN<32>) {
    env.storage()
        .instance()
        .set(&DataKey::VoucherWasmHash, &wasm_hash);
}

pub(crate) fn credit(env: &Env, denom: &String, to: &Address, amount: i128) {
    match try_details(env, denom) {
        Some(info) => match info.kind {
            AssetKind::Local => {
                let escrow = env.current_contract_address();
                token::Client::new(env, &info.address).transfer(&escrow, to, &amount);
            }
            AssetKind::Voucher => {
                token::StellarAssetClient::new(env, &info.address).mint(to, &amount);
            }
        },
        None => {
            let voucher = ensure_voucher(env, denom);
            voucher_mint(env, &voucher, to, amount);
        }
    }
}

pub(crate) fn debit(env: &Env, denom: &String, from: &Address, amount: i128) {
    match try_details(env, denom) {
        Some(info) => match info.kind {
            AssetKind::Local => {
                let escrow = env.current_contract_address();
                token::Client::new(env, &info.address).transfer(from, &escrow, &amount);
            }
            AssetKind::Voucher => {
                token::Client::new(env, &info.address).burn(from, &amount);
            }
        },
        None => {
            let voucher = voucher_address(env, denom);
            token::Client::new(env, &voucher).burn(from, &amount);
        }
    }
}

pub(crate) fn balance(env: &Env, who: &Address, denom: &String) -> i128 {
    token::Client::new(env, &token_address(env, denom)).balance(who)
}

pub(crate) fn token_address(env: &Env, denom: &String) -> Address {
    match try_details(env, denom) {
        Some(info) => info.address,
        None => voucher_address(env, denom),
    }
}

pub(crate) fn voucher_address(env: &Env, denom: &String) -> Address {
    env.deployer()
        .with_current_contract(voucher_salt(env, denom))
        .deployed_address()
}

fn ensure_voucher(env: &Env, denom: &String) -> Address {
    let salt = voucher_salt(env, denom);
    let address = env
        .deployer()
        .with_current_contract(salt.clone())
        .deployed_address();

    if !env
        .storage()
        .persistent()
        .has(&DataKey::Voucher(denom.clone()))
    {
        let wasm_hash: BytesN<32> = env
            .storage()
            .instance()
            .get(&DataKey::VoucherWasmHash)
            .unwrap_or_else(|| panic_with_error!(env, Error::VoucherWasmNotSet));
        let owner = env.current_contract_address();
        let args = (owner, 0u32, denom.clone(), denom.clone());

        env.deployer()
            .with_current_contract(salt)
            .deploy_v2(wasm_hash, args);
        env.storage()
            .persistent()
            .set(&DataKey::Voucher(denom.clone()), &true);
    }

    address
}

fn voucher_salt(env: &Env, denom: &String) -> BytesN<32> {
    let len = denom.len() as usize;
    if len > MAX_DENOM_BYTES {
        panic_with_error!(env, Error::InvalidPacketData);
    }

    let mut buf = [0u8; MAX_DENOM_BYTES];
    denom.copy_into_slice(&mut buf[..len]);
    let bytes = Bytes::from_slice(env, &buf[..len]);

    env.crypto().sha256(&bytes).to_bytes()
}

fn voucher_mint(env: &Env, voucher: &Address, to: &Address, amount: i128) {
    env.invoke_contract::<()>(
        voucher,
        &Symbol::new(env, "mint"),
        vec![env, to.into_val(env), amount.into_val(env)],
    );
}

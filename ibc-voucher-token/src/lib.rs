#![no_std]

use soroban_sdk::{contract, contractimpl, Address, Env, MuxedAddress, String};
use stellar_access::ownable::{set_owner, Ownable};
use stellar_macros::only_owner;
use stellar_tokens::fungible::{burnable::FungibleBurnable, Base, FungibleToken};

#[contract]
pub struct VoucherToken;

#[contractimpl]
impl VoucherToken {
    pub fn __constructor(env: Env, owner: Address, decimal: u32, name: String, symbol: String) {
        set_owner(&env, &owner);
        Base::set_metadata(&env, decimal, name, symbol);
    }

    #[only_owner]
    pub fn mint(env: Env, to: Address, amount: i128) {
        Base::mint(&env, &to, amount);
    }
}

#[contractimpl(contracttrait)]
impl Ownable for VoucherToken {}

#[contractimpl(contracttrait)]
impl FungibleToken for VoucherToken {
    type ContractType = Base;
}

#[contractimpl(contracttrait)]
impl FungibleBurnable for VoucherToken {}

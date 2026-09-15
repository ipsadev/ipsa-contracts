#![no_std]

use soroban_sdk::{
    auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
    contract, contractimpl, vec, Address, Env, Symbol, Val, Vec,
};
use stellar_access::ownable::{set_owner, Ownable};
use stellar_macros::only_owner;

#[contract]
pub struct IpsaGmpAccount;

#[contractimpl(contracttrait)]
impl Ownable for IpsaGmpAccount {}

#[contractimpl]
impl IpsaGmpAccount {
    pub fn __constructor(env: Env, owner: Address) {
        set_owner(&env, &owner);
    }

    #[only_owner]
    pub fn execute(env: Env, target: Address, function: Symbol, args: Vec<Val>) -> Val {
        env.authorize_as_current_contract(vec![
            &env,
            InvokerContractAuthEntry::Contract(SubContractInvocation {
                context: ContractContext {
                    contract: target.clone(),
                    fn_name: function.clone(),
                    args: args.clone(),
                },
                sub_invocations: vec![&env],
            }),
        ]);

        env.invoke_contract(&target, &function, args)
    }
}

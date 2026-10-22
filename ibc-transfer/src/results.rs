use soroban_sdk::{vec, Address, Env, IntoVal, String, Symbol, Val};

use crate::types::{TransferAcknowledged, TransferTimedOut};

const CONTRACT_ADDRESS_LENGTH: usize = 56;

const ACKNOWLEDGEMENT_FUNCTION: &str = "on_transfer_acknowledgement";

const TIMEOUT_FUNCTION: &str = "on_transfer_timeout";

pub(crate) fn acknowledged(env: &Env, sender: &String, result: TransferAcknowledged) {
    if let Some(contract) = contract_sender(sender) {
        notify(
            env,
            &contract,
            ACKNOWLEDGEMENT_FUNCTION,
            result.into_val(env),
        );
    }
}

pub(crate) fn timed_out(env: &Env, sender: &String, result: TransferTimedOut) {
    if let Some(contract) = contract_sender(sender) {
        notify(env, &contract, TIMEOUT_FUNCTION, result.into_val(env));
    }
}

fn contract_sender(sender: &String) -> Option<Address> {
    if sender.len() as usize != CONTRACT_ADDRESS_LENGTH {
        return None;
    }

    let mut bytes = [0u8; CONTRACT_ADDRESS_LENGTH];
    sender.copy_into_slice(&mut bytes);

    if bytes[0] != b'C' {
        return None;
    }

    Some(Address::from_string(sender))
}

fn notify(env: &Env, contract: &Address, function: &str, result: Val) {
    let _ = env.try_invoke_contract::<(), soroban_sdk::Error>(
        contract,
        &Symbol::new(env, function),
        vec![env, env.current_contract_address().into_val(env), result],
    );
}

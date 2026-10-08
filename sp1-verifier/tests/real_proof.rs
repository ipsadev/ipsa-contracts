use soroban_sdk::{Bytes, BytesN, Env};
use stellar_sp1_verifier::{verify_proof, VerifyError};

const FIXTURE: &str = include_str!("./fixtures/proof.json");

fn field(name: &str) -> alloc::vec::Vec<u8> {
    let key = alloc::format!("\"{name}\": \"0x");
    let start = FIXTURE.find(&key).expect("field missing") + key.len();
    let end = FIXTURE[start..].find('"').expect("unterminated") + start;

    let hexed = &FIXTURE[start..end];

    (0..hexed.len() / 2)
        .map(|i| u8::from_str_radix(&hexed[i * 2..i * 2 + 2], 16).unwrap())
        .collect()
}

extern crate alloc;

struct Case {
    vkey: [u8; 32],
    public_values: alloc::vec::Vec<u8>,
    proof: alloc::vec::Vec<u8>,
}

fn load() -> Case {
    Case {
        vkey: field("vkey").try_into().expect("vkey is not 32 bytes"),
        public_values: field("publicValues"),
        proof: field("proof"),
    }
}

#[test]
fn a_real_sp1_proof_verifies_on_soroban() {
    let env = Env::default();
    let case = load();

    env.cost_estimate().budget().reset_default();

    let result = verify_proof(
        &env,
        &BytesN::from_array(&env, &case.vkey),
        &Bytes::from_slice(&env, &case.public_values),
        &Bytes::from_slice(&env, &case.proof),
    );

    std::println!(
        "  cpu {} ({:.1}% of a transaction), mem {}",
        env.cost_estimate().budget().cpu_instruction_cost(),
        100.0 * env.cost_estimate().budget().cpu_instruction_cost() as f64 / 100_000_000.0,
        env.cost_estimate().budget().memory_bytes_cost()
    );

    assert_eq!(result, Ok(()), "a real SP1 proof must verify on Soroban");
}

#[test]
fn a_proof_of_another_program_is_rejected() {
    let env = Env::default();
    let case = load();

    let mut other = case.vkey;

    other[31] ^= 0x01;

    assert_eq!(
        verify_proof(
            &env,
            &BytesN::from_array(&env, &other),
            &Bytes::from_slice(&env, &case.public_values),
            &Bytes::from_slice(&env, &case.proof),
        ),
        Err(VerifyError::PairingFailed)
    );
}

#[test]
fn tampered_public_values_are_rejected() {
    let env = Env::default();
    let case = load();

    let mut values = case.public_values.clone();

    *values.last_mut().unwrap() ^= 0x01;

    assert_eq!(
        verify_proof(
            &env,
            &BytesN::from_array(&env, &case.vkey),
            &Bytes::from_slice(&env, &values),
            &Bytes::from_slice(&env, &case.proof),
        ),
        Err(VerifyError::PairingFailed)
    );
}

#[test]
fn a_proof_with_the_wrong_selector_is_rejected() {
    let env = Env::default();
    let case = load();

    let mut proof = case.proof.clone();

    proof[0] ^= 0x01;

    assert_eq!(
        verify_proof(
            &env,
            &BytesN::from_array(&env, &case.vkey),
            &Bytes::from_slice(&env, &case.public_values),
            &Bytes::from_slice(&env, &proof),
        ),
        Err(VerifyError::WrongSelector)
    );
}

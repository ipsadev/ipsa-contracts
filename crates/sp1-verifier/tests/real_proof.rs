use soroban_sdk::{Bytes, BytesN, Env};
use stellar_sp1_verifier::{verify_proof, vk_root, VerifyError};

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

const VK_ROOT_WORD: core::ops::Range<usize> = 4 + 32..4 + 64;

const BN254_SCALAR_MODULUS: [u8; 32] = [
    0x30, 0x64, 0x4e, 0x72, 0xe1, 0x31, 0xa0, 0x29, 0xb8, 0x50, 0x45, 0xb6, 0x81, 0x81, 0x58, 0x5d,
    0x28, 0x33, 0xe8, 0x48, 0x79, 0xb9, 0x70, 0x91, 0x43, 0xe1, 0xf5, 0x93, 0xf0, 0x00, 0x00, 0x01,
];

fn add_be(a: &[u8], b: &[u8; 32]) -> [u8; 32] {
    let mut out = [0u8; 32];
    let mut carry = 0u16;

    for i in (0..32).rev() {
        let sum = u16::from(a[i]) + u16::from(b[i]) + carry;
        out[i] = sum as u8;
        carry = sum >> 8;
    }

    out
}

fn verify_with(env: &Env, case: &Case, proof: &[u8]) -> Result<(), VerifyError> {
    verify_proof(
        env,
        &BytesN::from_array(env, &case.vkey),
        &Bytes::from_slice(env, &case.public_values),
        &Bytes::from_slice(env, proof),
    )
}

#[test]
fn the_real_proof_carries_the_pinned_vk_root() {
    let case = load();

    assert_eq!(&case.proof[VK_ROOT_WORD], vk_root().as_slice());
}

#[test]
fn a_proof_under_another_vk_root_is_rejected_before_the_pairing() {
    let env = Env::default();
    let case = load();
    let mut proof = case.proof.clone();

    proof[VK_ROOT_WORD.start + 31] ^= 0x01;

    env.cost_estimate().budget().reset_default();
    let result = verify_with(&env, &case, &proof);
    let cpu = env.cost_estimate().budget().cpu_instruction_cost();

    assert_eq!(result, Err(VerifyError::InvalidVkRoot));
    assert!(
        cpu < 5_000_000,
        "the check must run before the pairing, which costs tens of millions; got {cpu}"
    );
}

#[test]
fn a_vk_root_shifted_by_the_field_modulus_is_rejected() {
    let env = Env::default();
    let case = load();
    let mut proof = case.proof.clone();
    let shifted = add_be(&case.proof[VK_ROOT_WORD], &BN254_SCALAR_MODULUS);

    proof[VK_ROOT_WORD].copy_from_slice(&shifted);

    assert_eq!(
        verify_with(&env, &case, &proof),
        Err(VerifyError::InvalidVkRoot)
    );
}

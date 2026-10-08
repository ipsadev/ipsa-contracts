use soroban_sdk::{testutils::Address as _, Address, Bytes, BytesN, Env};
use stellar_ibc_router::{
    smt::{key_index, remove_leaf, set_leaf, tree_root, SmtKey},
    IbcRouter,
};

fn router(env: &Env) -> Address {
    env.register(IbcRouter, (Address::generate(env),))
}

fn key(env: &Env) -> Bytes {
    Bytes::from_slice(env, b"07-tendermint-0\x01\x00\x00\x00\x00\x00\x00\x00\x07")
}

fn value(env: &Env, byte: u8) -> Bytes {
    Bytes::from_slice(env, &[byte; 32])
}

fn other_owner(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0xee; 32])
}

#[test]
fn a_free_slot_is_claimed_by_the_first_key() {
    let env = Env::default();
    let id = router(&env);

    env.as_contract(&id, || {
        set_leaf(&env, &key(&env), &value(&env, 1));

        let index = key_index(&env, &key(&env));
        let owner: BytesN<32> = env
            .storage()
            .persistent()
            .get(&SmtKey::Owner(index))
            .unwrap();

        assert_eq!(owner, env.crypto().sha256(&key(&env)).to_bytes());
    });
}

#[test]
fn the_owner_can_overwrite_its_own_slot() {
    let env = Env::default();
    let id = router(&env);

    env.as_contract(&id, || {
        set_leaf(&env, &key(&env), &value(&env, 1));
        let before = tree_root(&env);

        set_leaf(&env, &key(&env), &value(&env, 2));

        assert_ne!(tree_root(&env), before);
    });
}

#[test]
#[should_panic(expected = "Error(Contract, #25)")]
fn a_colliding_key_cannot_take_an_occupied_slot() {
    let env = Env::default();
    let id = router(&env);

    env.as_contract(&id, || {
        let index = key_index(&env, &key(&env));
        env.storage()
            .persistent()
            .set(&SmtKey::Owner(index), &other_owner(&env));

        set_leaf(&env, &key(&env), &value(&env, 1));
    });
}

#[test]
fn a_colliding_key_cannot_remove_another_keys_leaf() {
    let env = Env::default();
    let id = router(&env);

    env.as_contract(&id, || {
        set_leaf(&env, &key(&env), &value(&env, 1));
        let index = key_index(&env, &key(&env));
        let root = tree_root(&env);

        env.storage()
            .persistent()
            .set(&SmtKey::Owner(index), &other_owner(&env));
        remove_leaf(&env, &key(&env));

        assert!(env.storage().persistent().has(&SmtKey::Leaf(index)));
        assert_eq!(tree_root(&env), root, "the receipt or commitment survives");
    });
}

#[test]
fn the_owner_removes_its_leaf_and_frees_the_slot() {
    let env = Env::default();
    let id = router(&env);

    env.as_contract(&id, || {
        set_leaf(&env, &key(&env), &value(&env, 1));
        let index = key_index(&env, &key(&env));

        remove_leaf(&env, &key(&env));

        assert!(!env.storage().persistent().has(&SmtKey::Leaf(index)));
        assert!(!env.storage().persistent().has(&SmtKey::Owner(index)));
        assert_eq!(
            tree_root(&env),
            soroban_sdk::BytesN::from_array(&env, &[0; 32])
        );
    });
}

#[test]
fn removing_an_empty_slot_changes_nothing() {
    let env = Env::default();
    let id = router(&env);

    env.as_contract(&id, || {
        remove_leaf(&env, &key(&env));

        assert_eq!(
            tree_root(&env),
            soroban_sdk::BytesN::from_array(&env, &[0; 32])
        );
    });
}

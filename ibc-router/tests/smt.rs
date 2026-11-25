use ipsa_smt::{empty, key_index, leaf_hash, Branch, Child};
use soroban_sdk::{testutils::Address as _, xdr::ToXdr, Address, Bytes, BytesN, Env};
use stellar_ibc_router::{smt, smt::SmtKey, IbcRouter, IbcRouterClient};

fn hex(bytes: &Bytes) -> String {
    use core::fmt::Write;

    let mut out = String::new();

    for i in 0..bytes.len() {
        let _ = write!(out, "{:02x}", bytes.get(i).unwrap());
    }

    out
}

fn deploy(env: &Env) -> (Address, IbcRouterClient<'static>) {
    let owner = Address::generate(env);
    let id = env.register(IbcRouter, (owner,));

    (id.clone(), IbcRouterClient::new(env, &id))
}

#[test]
fn the_child_encoding_is_unchanged() {
    let env = Env::default();

    assert_eq!(
        hex(&Child::Leaf(7).to_xdr(&env)),
        "0000001000000001000000020000000f000000044c656166000000050000000000000007"
    );
    assert_eq!(
        hex(&Child::Node(5, 42).to_xdr(&env)),
        "0000001000000001000000030000000f000000044e6f6465000000030000000500000005000000000000002a"
    );
}

#[test]
fn the_branch_encoding_is_unchanged() {
    let env = Env::default();
    let branch = Branch {
        hash: BytesN::from_array(&env, &[0xab; 32]),
        left: Child::Leaf(7),
        right: Child::Node(5, 42),
    };

    assert_eq!(
        hex(&branch.to_xdr(&env)),
        "0000001100000001000000030000000f00000004686173680000000d00000020\
abababababababababababababababababababababababababababababababab\
0000000f000000046c6566740000001000000001000000020000000f000000044c656166000000050000000000000007\
0000000f0000000572696768740000000000001000000001000000030000000f000000044e6f6465000000030000000500000005000000000000002a"
    );
}

#[test]
fn the_storage_key_discriminants_are_unchanged() {
    let env = Env::default();
    let keys: [(SmtKey, &str); 5] = [
        (SmtKey::Root, "Root"),
        (SmtKey::Top, "Top"),
        (SmtKey::Leaf(7), "Leaf"),
        (SmtKey::Owner(7), "Owner"),
        (SmtKey::Node(5, 42), "Node"),
    ];

    for (key, name) in keys {
        let encoded = hex(&key.to_xdr(&env));
        let tag = hex(&Bytes::from_slice(&env, name.as_bytes()));

        assert!(
            encoded.contains(&tag),
            "{name} must stay the encoded discriminant"
        );
    }
}

#[test]
fn a_committed_root_lags_the_tree_until_commit() {
    let env = Env::default();
    let (id, client) = deploy(&env);
    let key = Bytes::from_slice(&env, b"10-stellar-0");
    let value = Bytes::from_slice(&env, &[0xAB; 32]);

    assert_eq!(client.ibc_root(), empty(&env));

    env.as_contract(&id, || smt::set_leaf(&env, &key, &value));

    assert_eq!(
        client.ibc_root(),
        empty(&env),
        "writing a leaf does not move the committed root"
    );

    let committed = client.commit_root();

    assert_ne!(committed, empty(&env));
    assert_eq!(client.ibc_root(), committed);
}

#[test]
fn the_committed_root_matches_the_leaf_hash_of_a_single_entry() {
    let env = Env::default();
    let (id, client) = deploy(&env);
    let key = Bytes::from_slice(&env, b"10-stellar-0");
    let value = Bytes::from_slice(&env, &[0xAB; 32]);

    env.as_contract(&id, || smt::set_leaf(&env, &key, &value));
    client.commit_root();

    let expected = env.as_contract(&id, || {
        let index = key_index(&env, &key);
        let stored: BytesN<32> = env
            .storage()
            .persistent()
            .get(&SmtKey::Leaf(index))
            .unwrap();

        stored
    });

    assert_eq!(expected, leaf_hash(&env, &key, &value));
}

#[test]
fn removing_a_leaf_returns_the_committed_root_to_zero() {
    let env = Env::default();
    let (id, client) = deploy(&env);
    let key = Bytes::from_slice(&env, b"10-stellar-0");
    let value = Bytes::from_slice(&env, &[0xAB; 32]);

    env.as_contract(&id, || smt::set_leaf(&env, &key, &value));
    assert_ne!(client.commit_root(), empty(&env));

    env.as_contract(&id, || smt::remove_leaf(&env, &key));

    assert_eq!(client.commit_root(), empty(&env));
}

#[test]
fn a_slot_owned_by_another_key_is_refused() {
    let env = Env::default();
    let (id, _client) = deploy(&env);
    let key = Bytes::from_slice(&env, b"10-stellar-0");
    let value = Bytes::from_slice(&env, &[0xAB; 32]);

    let refused = env.as_contract(&id, || {
        let index = key_index(&env, &key);
        let stranger = env
            .crypto()
            .sha256(&Bytes::from_slice(&env, b"another key"))
            .to_bytes();

        env.storage()
            .persistent()
            .set(&SmtKey::Owner(index), &stranger);

        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            smt::set_leaf(&env, &key, &value)
        }))
        .is_err()
    });

    assert!(refused, "a key must not write over a slot another key owns");
}

#[test]
fn a_stranger_cannot_remove_a_leaf_it_does_not_own() {
    let env = Env::default();
    let (id, client) = deploy(&env);
    let key = Bytes::from_slice(&env, b"10-stellar-0");
    let value = Bytes::from_slice(&env, &[0xAB; 32]);

    env.as_contract(&id, || smt::set_leaf(&env, &key, &value));
    let committed = client.commit_root();

    env.as_contract(&id, || {
        let index = key_index(&env, &key);
        let stranger = env
            .crypto()
            .sha256(&Bytes::from_slice(&env, b"another key"))
            .to_bytes();

        env.storage()
            .persistent()
            .set(&SmtKey::Owner(index), &stranger);

        smt::remove_leaf(&env, &key);
    });

    assert_eq!(
        client.commit_root(),
        committed,
        "the leaf stays when the remover does not own the slot"
    );
}

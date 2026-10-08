use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Map};
use stellar_ibc_router::{
    smt::{compute_root, empty, tree_remove, tree_root, tree_set},
    IbcRouter,
};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

struct Tree {
    env: Env,
    id: Address,
}

impl Tree {
    fn new() -> Self {
        let env = Env::default();
        env.cost_estimate().budget().reset_unlimited();
        let id = env.register(IbcRouter, (Address::generate(&env),));

        Self { env, id }
    }

    fn set(&self, index: u64, value: &BytesN<32>) {
        self.env
            .as_contract(&self.id, || tree_set(&self.env, index, value));
    }

    fn remove(&self, index: u64) {
        self.env
            .as_contract(&self.id, || tree_remove(&self.env, index));
    }

    fn root(&self) -> BytesN<32> {
        self.env.as_contract(&self.id, || tree_root(&self.env))
    }

    fn check(&self, mirror: &Map<u64, BytesN<32>>) {
        assert_eq!(
            self.root(),
            compute_root(&self.env, mirror),
            "the incremental root must equal the reference root over {} leaves",
            mirror.len()
        );
    }
}

fn hash(env: &Env, seed: u64) -> BytesN<32> {
    let mut bytes = [0u8; 32];
    bytes[..8].copy_from_slice(&seed.to_be_bytes());
    bytes[31] = 0x5a;
    BytesN::from_array(env, &bytes)
}

#[test]
fn random_inserts_updates_and_removals_match_the_reference_root() {
    let tree = Tree::new();
    let env = &tree.env;
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut mirror: Map<u64, BytesN<32>> = Map::new(env);
    let mut keys: std::vec::Vec<u64> = std::vec::Vec::new();

    for step in 0..400u64 {
        let roll = rng.next() % 10;

        if roll < 6 || keys.is_empty() {
            let index = rng.next();
            let value = hash(env, step);
            tree.set(index, &value);
            mirror.set(index, value);
            keys.push(index);
        } else if roll < 8 {
            let index = keys[(rng.next() as usize) % keys.len()];
            let value = hash(env, step ^ 0xffff);
            tree.set(index, &value);
            mirror.set(index, value);
        } else {
            let at = (rng.next() as usize) % keys.len();
            let index = keys.swap_remove(at);
            tree.remove(index);
            mirror.remove(index);
        }

        if step % 20 == 0 {
            tree.check(&mirror);
        }
    }

    tree.check(&mirror);

    for index in keys {
        tree.remove(index);
        mirror.remove(index);
    }

    assert_eq!(tree.root(), empty(env));
}

#[test]
fn edge_indices_match_the_reference_root() {
    let tree = Tree::new();
    let env = &tree.env;
    let indices = [
        0u64,
        1,
        2,
        3,
        u64::MAX,
        u64::MAX - 1,
        1 << 63,
        (1 << 63) - 1,
        0x0000_0001_0000_0000,
        0x8000_0000_0000_0001,
    ];
    let mut mirror: Map<u64, BytesN<32>> = Map::new(env);

    for (n, index) in indices.iter().enumerate() {
        let value = hash(env, n as u64);
        tree.set(*index, &value);
        mirror.set(*index, value);
        tree.check(&mirror);
    }

    for index in indices.iter().rev() {
        tree.remove(*index);
        mirror.remove(*index);
        tree.check(&mirror);
    }

    assert_eq!(tree.root(), empty(env));
}

#[test]
fn a_single_leaf_and_its_removal() {
    let tree = Tree::new();
    let env = &tree.env;
    let mut mirror: Map<u64, BytesN<32>> = Map::new(env);

    tree.set(42, &hash(env, 1));
    mirror.set(42, hash(env, 1));
    tree.check(&mirror);

    tree.remove(42);
    assert_eq!(tree.root(), empty(env));

    tree.remove(42);
    assert_eq!(tree.root(), empty(env), "removing twice is harmless");
}

#[test]
fn one_write_into_a_large_tree_fits_one_transaction() {
    let tree = Tree::new();
    let env = &tree.env;
    let mut rng = Rng(0xdead_beef_cafe_f00d);

    for n in 0..1_000u64 {
        tree.set(rng.next(), &hash(env, n));
    }

    let index = rng.next();

    env.cost_estimate().budget().reset_default();
    tree.set(index, &hash(env, 1));
    let insert = env.cost_estimate().budget().cpu_instruction_cost();

    env.cost_estimate().budget().reset_default();
    tree.remove(index);
    let remove = env.cost_estimate().budget().cpu_instruction_cost();

    std::println!("1,000 leaves: insert {insert} cpu, remove {remove} cpu");

    assert!(
        insert < 30_000_000 && remove < 30_000_000,
        "a write must stay far below the 100M transaction budget: insert {insert}, remove {remove}"
    );
}

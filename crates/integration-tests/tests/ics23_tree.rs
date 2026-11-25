use integration_tests::ics23::{iavl_tree, iavl_tree_n};

const LEAVES: [(&[u8], &[u8]); 4] = [
    (b"alpha", &[0x01; 32]),
    (b"bravo", &[0x02; 32]),
    (b"charlie", &[0x03; 32]),
    (b"delta", &[0x04; 32]),
];

#[test]
fn the_general_builder_agrees_with_the_four_leaf_builder() {
    let fixed = iavl_tree(&LEAVES);
    let general = iavl_tree_n(&LEAVES);

    assert_eq!(
        fixed.root, general.root,
        "the same four leaves must give the same root"
    );

    for (i, (a, b)) in fixed.proofs.iter().zip(general.proofs.iter()).enumerate() {
        assert_eq!(a.key, b.key, "leaf {i} key");
        assert_eq!(a.value, b.value, "leaf {i} value");
        assert_eq!(a.leaf, b.leaf, "leaf {i} leaf op");
        assert_eq!(a.path.len(), b.path.len(), "leaf {i} path length");

        for (depth, (x, y)) in a.path.iter().zip(b.path.iter()).enumerate() {
            assert_eq!(x.prefix, y.prefix, "leaf {i} inner {depth} prefix");
            assert_eq!(x.suffix, y.suffix, "leaf {i} inner {depth} suffix");
        }
    }
}

#[test]
fn a_tree_is_built_for_leaf_counts_that_are_not_powers_of_two() {
    for count in [1usize, 2, 3, 5, 7, 8, 9] {
        let owned: std::vec::Vec<(std::vec::Vec<u8>, [u8; 32])> = (0..count)
            .map(|i| (format!("key-{i}").into_bytes(), [i as u8 + 1; 32]))
            .collect();
        let leaves: std::vec::Vec<(&[u8], &[u8])> = owned
            .iter()
            .map(|(k, v)| (k.as_slice(), v.as_slice()))
            .collect();

        let tree = iavl_tree_n(&leaves);

        assert_eq!(
            tree.proofs.len(),
            count,
            "{count} leaves should give {count} proofs, and no proof for the padding"
        );
    }
}

#[test]
fn every_leaf_gets_a_distinct_proof() {
    let tree = iavl_tree_n(&LEAVES);

    for i in 0..tree.proofs.len() {
        for j in (i + 1)..tree.proofs.len() {
            assert_ne!(
                tree.proofs[i].key, tree.proofs[j].key,
                "proofs {i} and {j} describe the same leaf"
            );
        }
    }
}

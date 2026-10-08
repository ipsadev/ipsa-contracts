mod bls_sync_committee {
    use soroban_sdk::{
        crypto::bls12_381::Bls12381G1Affine, testutils::budget::Budget, Bytes, Env, Vec,
    };

    const COMMITTEE: u32 = 512;
    const SIG_DST: &[u8] = b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_";
    const KEY_DST: &[u8] = b"BLS_SPIKE_KEYGEN_ONLY_NOT_A_REAL_KEY_";

    fn committee(env: &Env, size: u32) -> Vec<Bls12381G1Affine> {
        let bls = env.crypto().bls12_381();
        let dst = Bytes::from_slice(env, KEY_DST);

        let mut keys = Vec::new(env);

        for index in 0..size {
            let msg = Bytes::from_array(env, &index.to_be_bytes());

            keys.push_back(bls.hash_to_g1(&msg, &dst));
        }

        keys
    }

    fn report(label: &str, budget: &Budget) {
        std::println!(
            "  {label:<38} {:>14} cpu  {:>12} mem",
            budget.cpu_instruction_cost(),
            budget.memory_bytes_cost()
        );
    }

    #[test]
    fn what_a_sync_committee_verification_costs_on_soroban() {
        let env = Env::default();

        env.cost_estimate().budget().reset_unlimited();

        let keys = committee(&env, COMMITTEE);

        let bls = env.crypto().bls12_381();
        let sig_dst = Bytes::from_slice(&env, SIG_DST);
        let signing_root = Bytes::from_array(&env, &[0x5a; 32]);

        std::println!();
        std::println!("Soroban BLS12-381, {COMMITTEE}-member sync committee");
        std::println!();

        env.cost_estimate().budget().reset_default();

        let mut aggregate = keys.get(0).unwrap();

        for index in 1..COMMITTEE {
            aggregate = bls.g1_add(&aggregate, &keys.get(index).unwrap());
        }

        report(
            "aggregate 512 pubkeys (g1_add)",
            &env.cost_estimate().budget(),
        );

        env.cost_estimate().budget().reset_default();

        let message = bls.hash_to_g2(&signing_root, &sig_dst);

        report("hash_to_g2(signing root)", &env.cost_estimate().budget());

        env.cost_estimate().budget().reset_default();

        let mut g1s = Vec::new(&env);
        let mut g2s = Vec::new(&env);

        g1s.push_back(aggregate.clone());
        g1s.push_back(keys.get(1).unwrap());
        g2s.push_back(message.clone());
        g2s.push_back(message.clone());

        let _ = bls.pairing_check(g1s, g2s);

        report("pairing_check (2 pairs)", &env.cost_estimate().budget());

        env.cost_estimate().budget().reset_default();

        let mut total = keys.get(0).unwrap();

        for index in 1..COMMITTEE {
            total = bls.g1_add(&total, &keys.get(index).unwrap());
        }

        let msg = bls.hash_to_g2(&signing_root, &sig_dst);

        let mut p1 = Vec::new(&env);
        let mut p2 = Vec::new(&env);

        p1.push_back(total);
        p1.push_back(keys.get(1).unwrap());
        p2.push_back(msg.clone());
        p2.push_back(msg);

        let _ = bls.pairing_check(p1, p2);

        std::println!();
        report("WHOLE VERIFICATION", &env.cost_estimate().budget());
        std::println!();
    }

    #[test]
    fn how_aggregation_scales_with_participation() {
        let env = Env::default();

        env.cost_estimate().budget().reset_unlimited();

        let keys = committee(&env, COMMITTEE);
        let bls = env.crypto().bls12_381();

        std::println!();
        std::println!("Aggregation cost by number of participating signers");
        std::println!();

        for size in [1_u32, 64, 128, 256, 384, 512] {
            env.cost_estimate().budget().reset_default();

            let mut aggregate = keys.get(0).unwrap();

            for index in 1..size {
                aggregate = bls.g1_add(&aggregate, &keys.get(index).unwrap());
            }

            std::println!(
                "  {size:>4} signers {:>14} cpu  {:>12} mem",
                env.cost_estimate().budget().cpu_instruction_cost(),
                env.cost_estimate().budget().memory_bytes_cost()
            );
        }

        std::println!();
    }
}

mod bls_msm_is_a_trap {
    use soroban_sdk::{
        crypto::bls12_381::{Bls12381Fr, Bls12381G1Affine},
        Bytes, Env, Vec,
    };

    const COMMITTEE: u32 = 512;
    const KEY_DST: &[u8] = b"BLS_SPIKE_KEYGEN_ONLY_NOT_A_REAL_KEY_";
    const CPU_LIMIT: u64 = 100_000_000;

    fn committee(env: &Env, size: u32) -> Vec<Bls12381G1Affine> {
        let bls = env.crypto().bls12_381();
        let dst = Bytes::from_slice(env, KEY_DST);

        let mut keys = Vec::new(env);

        for index in 0..size {
            keys.push_back(bls.hash_to_g1(&Bytes::from_array(env, &index.to_be_bytes()), &dst));
        }

        keys
    }

    fn line(label: &str, cpu: u64) {
        std::println!(
            "  {label:<44} {cpu:>12} cpu   {:>5.1}% of budget",
            100.0 * cpu as f64 / CPU_LIMIT as f64
        );
    }

    #[test]
    fn msm_versus_sequential_addition() {
        let env = Env::default();

        env.cost_estimate().budget().reset_unlimited();

        let keys = committee(&env, COMMITTEE);
        let bls = env.crypto().bls12_381();

        std::println!();
        std::println!("Aggregating {COMMITTEE} public keys, two ways");
        std::println!();

        env.cost_estimate().budget().reset_default();

        let mut sequential = keys.get(0).unwrap();

        for index in 1..COMMITTEE {
            sequential = bls.g1_add(&sequential, &keys.get(index).unwrap());
        }

        line(
            "512 x g1_add",
            env.cost_estimate().budget().cpu_instruction_cost(),
        );

        let one = Bls12381Fr::from_u256(soroban_sdk::U256::from_u32(&env, 1));

        let mut scalars = Vec::new(&env);

        for _ in 0..COMMITTEE {
            scalars.push_back(one.clone());
        }

        env.cost_estimate().budget().reset_unlimited();
        env.cost_estimate().budget().reset_tracker();

        let msm = bls.g1_msm(keys.clone(), scalars);

        line(
            "g1_msm(512 points, all scalars = 1)",
            env.cost_estimate().budget().cpu_instruction_cost(),
        );

        assert_eq!(sequential, msm, "both routes must agree on the aggregate");

        std::println!();
    }
}

mod groth16_over_bn254 {
    use soroban_sdk::{
        crypto::bn254::{Bn254Fr, Bn254G1Affine, Bn254G2Affine},
        BytesN, Env, Vec, U256,
    };

    const CPU_LIMIT: u64 = 100_000_000;

    fn line(label: &str, cpu: u64, mem: u64) {
        std::println!(
            "  {label:<46} {cpu:>11} cpu {mem:>8} mem  {:>5.1}%",
            100.0 * cpu as f64 / CPU_LIMIT as f64
        );
    }

    fn g1(env: &Env) -> Bn254G1Affine {
        let mut raw = [0_u8; 64];

        raw[31] = 1;
        raw[63] = 2;

        Bn254G1Affine::from_bytes(BytesN::from_array(env, &raw))
    }

    fn g2(env: &Env) -> Bn254G2Affine {
        let x_c0 = hex_32("1800deef121f1e76426a00665e5c4479674322d4f75edadd46debd5cd992f6ed");
        let x_c1 = hex_32("198e9393920d483a7260bfb731fb5d25f1aa493335a9e71297e485b7aef312c2");
        let y_c0 = hex_32("12c85ea5db8c6deb4aab71808dcb408fe3d1e7690c43d37b4ce6cc0166fa7daa");
        let y_c1 = hex_32("090689d0585ff075ec9e99ad690c3395bc4b313370b38ef355acdadcd122975b");

        let mut raw = [0_u8; 128];

        raw[..32].copy_from_slice(&x_c1);
        raw[32..64].copy_from_slice(&x_c0);
        raw[64..96].copy_from_slice(&y_c1);
        raw[96..].copy_from_slice(&y_c0);

        Bn254G2Affine::from_bytes(BytesN::from_array(env, &raw))
    }

    fn hex_32(hexed: &str) -> [u8; 32] {
        let mut out = [0_u8; 32];

        for (index, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hexed[index * 2..index * 2 + 2], 16).unwrap();
        }

        out
    }

    #[test]
    fn what_a_groth16_verification_costs_on_soroban() {
        let env = Env::default();

        let bn = env.crypto().bn254();
        let p1 = g1(&env);
        let p2 = g2(&env);

        std::println!();
        std::println!("Soroban BN254, shaped like a Groth16 verification");
        std::println!();

        for pairs in [2_usize, 3, 4] {
            env.cost_estimate().budget().reset_default();

            let mut vp1 = Vec::new(&env);
            let mut vp2 = Vec::new(&env);

            for _ in 0..pairs {
                vp1.push_back(p1.clone());
                vp2.push_back(p2.clone());
            }

            let _ = bn.pairing_check(vp1, vp2);

            line(
                &std::format!("pairing_check, {pairs} pairs"),
                env.cost_estimate().budget().cpu_instruction_cost(),
                env.cost_estimate().budget().memory_bytes_cost(),
            );
        }

        for inputs in [1_u32, 4, 8] {
            env.cost_estimate().budget().reset_default();

            let mut points = Vec::new(&env);
            let mut scalars = Vec::new(&env);

            for index in 0..inputs {
                points.push_back(p1.clone());
                scalars.push_back(Bn254Fr::from_u256(U256::from_u32(&env, index + 1)));
            }

            let _ = bn.g1_msm(points, scalars);

            line(
                &std::format!("g1_msm over {inputs} public inputs"),
                env.cost_estimate().budget().cpu_instruction_cost(),
                env.cost_estimate().budget().memory_bytes_cost(),
            );
        }

        env.cost_estimate().budget().reset_default();

        let mut points = Vec::new(&env);
        let mut scalars = Vec::new(&env);

        for index in 0..4_u32 {
            points.push_back(p1.clone());
            scalars.push_back(Bn254Fr::from_u256(U256::from_u32(&env, index + 1)));
        }

        let _ = bn.g1_msm(points, scalars);

        let mut vp1 = Vec::new(&env);
        let mut vp2 = Vec::new(&env);

        for _ in 0..4 {
            vp1.push_back(p1.clone());
            vp2.push_back(p2.clone());
        }

        let _ = bn.pairing_check(vp1, vp2);

        std::println!();
        line(
            "WHOLE GROTH16 VERIFICATION",
            env.cost_estimate().budget().cpu_instruction_cost(),
            env.cost_estimate().budget().memory_bytes_cost(),
        );
        std::println!();
    }
}

mod keccak_for_mpt_proofs {
    use soroban_sdk::{Bytes, Env};

    const CPU_LIMIT: u64 = 100_000_000;

    fn line(label: &str, cpu: u64, mem: u64) {
        std::println!(
            "  {label:<44} {cpu:>11} cpu {mem:>9} mem  {:>5.2}%",
            100.0 * cpu as f64 / CPU_LIMIT as f64
        );
    }

    fn measure(env: &Env, label: &str, run: impl Fn()) {
        env.cost_estimate().budget().reset_default();

        run();

        line(
            label,
            env.cost_estimate().budget().cpu_instruction_cost(),
            env.cost_estimate().budget().memory_bytes_cost(),
        );
    }

    #[test]
    fn what_keccak_costs_for_an_mpt_proof() {
        let env = Env::default();

        std::println!();
        std::println!("Soroban keccak256, sized for Ethereum MPT proofs");
        std::println!();

        let branch = Bytes::from_array(&env, &[0x5a; 532]);
        let small = Bytes::from_array(&env, &[0x5a; 32]);

        measure(&env, "keccak256(32 bytes)", || {
            let _ = env.crypto().keccak256(&small);
        });

        measure(&env, "keccak256(532 bytes, one branch node)", || {
            let _ = env.crypto().keccak256(&branch);
        });

        for depth in [6_u32, 9, 12] {
            measure(
                &env,
                &std::format!("one storage proof, depth {depth}"),
                || {
                    for _ in 0..depth {
                        let _ = env.crypto().keccak256(&branch);
                    }
                },
            );
        }

        measure(&env, "account proof + storage proof (9 + 9)", || {
            for _ in 0..18 {
                let _ = env.crypto().keccak256(&branch);
            }
        });

        std::println!();
        std::println!("For comparison, sha256 at the same sizes");
        std::println!();

        measure(&env, "sha256(532 bytes)", || {
            let _ = env.crypto().sha256(&branch);
        });

        measure(&env, "sha256 x 18 (532 bytes)", || {
            for _ in 0..18 {
                let _ = env.crypto().sha256(&branch);
            }
        });

        std::println!();
    }
}

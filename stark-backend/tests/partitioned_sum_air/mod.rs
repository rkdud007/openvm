use afs_stark_backend::{
    keygen::MultiStarkKeygenBuilder,
    prover::{trace::TraceCommitmentBuilder, MultiTraceStarkProver, USE_DEBUG_BUILDER},
    verifier::{MultiTraceStarkVerifier, VerificationError},
};
use itertools::Itertools;
use p3_baby_bear::BabyBear;
use p3_field::AbstractField;
use p3_matrix::dense::RowMajorMatrix;
use p3_util::log2_ceil_usize;
use rand::{rngs::StdRng, SeedableRng};

use crate::{config, utils::generate_random_matrix};

pub mod air;

use self::air::SumAir;

type Val = BabyBear;

// See air.rs for description of SumAir
fn prove_and_verify_sum_air(x: Vec<Val>, ys: Vec<Vec<Val>>) -> Result<(), VerificationError> {
    assert_eq!(x.len(), ys.len());
    let degree = x.len();
    let log_degree = log2_ceil_usize(degree);

    let perm = config::poseidon2::random_perm();
    let config = config::poseidon2::default_config(&perm, log_degree);

    let x_trace = RowMajorMatrix::new(x, 1);
    let y_width = ys[0].len();
    let y_trace = RowMajorMatrix::new(ys.into_iter().flatten().collect_vec(), y_width);

    let air = SumAir(y_width);

    let mut keygen_builder = MultiStarkKeygenBuilder::new(&config);
    let y_ptr = keygen_builder.add_cached_main_matrix(y_width);
    let x_ptr = keygen_builder.add_main_matrix(1);
    keygen_builder.add_partitioned_air(&air, degree, 0, vec![x_ptr, y_ptr]);
    let pk = keygen_builder.generate_pk();
    let vk = pk.vk();

    let prover = MultiTraceStarkProver::new(config);
    // Must add trace matrices in the same order as above
    let mut trace_builder = TraceCommitmentBuilder::new(prover.pcs());
    // Demonstrate y is cached
    let y_data = trace_builder.committer.commit(vec![y_trace.clone()]);
    trace_builder.load_cached_trace(y_trace, y_data);
    // Load x normally
    trace_builder.load_trace(x_trace);
    trace_builder.commit_current();

    let main_trace_data = trace_builder.view(&vk, vec![&air]);
    let pis = vec![vec![]];

    let mut challenger = config::poseidon2::Challenger::new(perm.clone());
    let proof = prover.prove(&mut challenger, &pk, main_trace_data, &pis);

    // Verify the proof:
    // Start from clean challenger
    let mut challenger = config::poseidon2::Challenger::new(perm.clone());
    let verifier = MultiTraceStarkVerifier::new(prover.config);
    verifier.verify(&mut challenger, vk, vec![&air], proof, &pis)
}

#[test]
fn test_partitioned_sum_air_happy_path() {
    let rng = StdRng::seed_from_u64(0);
    let n = 1 << 3;
    let ys = generate_random_matrix::<Val>(rng, n, 5);
    let x: Vec<Val> = ys
        .iter()
        .map(|row| row.iter().fold(Val::zero(), |sum, x| sum + *x))
        .collect();
    prove_and_verify_sum_air(x, ys).expect("Verification failed");
}

#[test]
fn test_partitioned_sum_air_happy_neg() {
    let rng = StdRng::seed_from_u64(0);
    let n = 1 << 3;
    let ys = generate_random_matrix(rng, n, 5);
    let mut x: Vec<Val> = ys
        .iter()
        .map(|row| row.iter().fold(Val::zero(), |sum, x| sum + *x))
        .collect();
    x[0] = Val::zero();
    USE_DEBUG_BUILDER.with(|debug| {
        *debug.lock().unwrap() = false;
    });
    assert_eq!(
        prove_and_verify_sum_air(x, ys),
        Err(VerificationError::OodEvaluationMismatch)
    );
}
use afs_compiler::prelude::*;
use p3_commit::TwoAdicMultiplicativeCoset;
use p3_field::{AbstractField, TwoAdicField};
use p3_symmetric::Hash;

use super::{
    types::{
        DimensionsVariable, FriConfigVariable, TwoAdicPcsMatsVariable, TwoAdicPcsProofVariable,
        TwoAdicPcsRoundVariable,
    },
    verify_batch, verify_challenges, verify_shape_and_sample_challenges, NestedOpenedValues,
    TwoAdicMultiplicativeCosetVariable,
};
use crate::{challenger::ChallengerVariable, commit::PcsVariable, digest::DigestVariable};

/// Notes:
/// 1. FieldMerkleTreeMMCS sorts traces by height in descending order when committing data.
/// Reference:
/// https://github.com/Plonky3/Plonky3/blob/27b3127dab047e07145c38143379edec2960b3e1/merkle-tree/src/merkle_tree.rs#L53
/// So traces are sorted in `opening_proof`.
/// 2. FieldMerkleTreeMMCS::verify_batch keeps the raw values in the original order. So traces are
/// not sorted in `opened_values`.
/// Reference:
/// https://github.com/Plonky3/Plonky3/blob/27b3127dab047e07145c38143379edec2960b3e1/merkle-tree/src/mmcs.rs#L87
/// https://github.com/Plonky3/Plonky3/blob/27b3127dab047e07145c38143379edec2960b3e1/merkle-tree/src/merkle_tree.rs#L100
pub fn verify_two_adic_pcs<C: Config>(
    builder: &mut Builder<C>,
    config: &FriConfigVariable<C>,
    rounds: Array<C, TwoAdicPcsRoundVariable<C>>,
    proof: TwoAdicPcsProofVariable<C>,
    challenger: &mut impl ChallengerVariable<C>,
) where
    C::F: TwoAdicField,
    C::EF: TwoAdicField,
{
    let g = builder.generator();

    let log_blowup = config.log_blowup;
    let blowup = config.blowup;
    let alpha = challenger.sample_ext(builder);
    let fri_proof = proof.fri_proof;

    builder.cycle_tracker_start("stage-d-1-verify-shape-and-sample-challenges");
    let fri_challenges =
        verify_shape_and_sample_challenges(builder, config, &fri_proof, challenger);
    builder.cycle_tracker_end("stage-d-1-verify-shape-and-sample-challenges");

    let log_global_max_height =
        builder.eval_expr(fri_proof.commit_phase_commits.len() + RVar::from(log_blowup));

    let reduced_openings: Array<_, Array<_, Ext<_, _>>> = builder.array(proof.query_openings.len());

    builder.cycle_tracker_start("stage-d-2-fri-fold");
    builder
        .range(0, proof.query_openings.len())
        .for_each(|i, builder| {
            let query_opening = builder.get(&proof.query_openings, i);
            let index_bits = builder.get(&fri_challenges.query_indices, i);

            let ro: Array<C, Ext<C::F, C::EF>> = builder.array(32);
            let alpha_pow: Array<C, Ext<C::F, C::EF>> = builder.array(32);
            if builder.flags.static_only {
                for j in 0..32 {
                    // ATTENTION: don't use set_value here, Fixed will share the same variable.
                    builder.set(&ro, j, C::EF::zero().cons());
                    builder.set(&alpha_pow, j, C::EF::one().cons());
                }
            } else {
                let zero_ef = builder.eval(C::EF::zero().cons());
                let one_ef = builder.eval(C::EF::one().cons());
                for j in 0..32 {
                    // Use set_value here to save a copy.
                    builder.set_value(&ro, j, zero_ef);
                    builder.set_value(&alpha_pow, j, one_ef);
                }
            }

            builder.range(0, rounds.len()).for_each(|j, builder| {
                let batch_opening = builder.get(&query_opening, j);
                let round = builder.get(&rounds, j);
                let batch_commit = round.batch_commit;
                let mats = round.mats;
                let permutation = round.permutation;
                let to_perm_index = |builder: &mut Builder<_>, k: RVar<_>| {
                    // Always no permutation in static mode
                    if builder.flags.static_only {
                        builder.eval(k)
                    } else {
                        let ret: Usize<_> = builder.uninit();
                        builder.if_eq(permutation.len(), RVar::zero()).then_or_else(
                            |builder| {
                                builder.assign(&ret, k);
                            },
                            |builder| {
                                let value = builder.get(&permutation, k);
                                builder.assign(&ret, value);
                            },
                        );
                        ret
                    }
                };

                let log_batch_max_height: Usize<_> = {
                    let log_batch_max_index = to_perm_index(builder, RVar::zero());
                    let mat = builder.get(&mats, log_batch_max_index);
                    let domain = mat.domain;
                    builder.eval(domain.log_n + RVar::from(log_blowup))
                };

                let batch_dims: Array<C, DimensionsVariable<C>> = builder.array(mats.len());
                // `verify_batch` requires `permed_opened_values` to be in the committed order.
                let permed_opened_values = builder.array(batch_opening.opened_values.len());
                builder.range(0, mats.len()).for_each(|k, builder| {
                    let mat_index = to_perm_index(builder, k);

                    let mat = builder.get(&mats, mat_index.clone());
                    let domain = mat.domain;
                    let dim = DimensionsVariable::<C> {
                        height: builder.eval(domain.size() * RVar::from(blowup)),
                    };
                    builder.set_value(&batch_dims, k, dim);
                    let opened_value = builder.get(&batch_opening.opened_values, mat_index);
                    builder.set_value(&permed_opened_values, k, opened_value);
                });
                let permed_opened_values = NestedOpenedValues::Felt(permed_opened_values);

                let bits_reduced: Usize<_> =
                    builder.eval(log_global_max_height - log_batch_max_height);
                let index_bits_shifted_v1 = index_bits.shift(builder, bits_reduced);

                builder.cycle_tracker_start("verify-batch");
                verify_batch::<C>(
                    builder,
                    &batch_commit,
                    batch_dims,
                    index_bits_shifted_v1,
                    &permed_opened_values,
                    &batch_opening.opening_proof,
                );
                builder.cycle_tracker_end("verify-batch");

                builder.cycle_tracker_start("compute-reduced-opening");
                // `verify_challenges` requires `opened_values` to be in the original order.
                let opened_values = batch_opening.opened_values;
                builder
                    .range(0, opened_values.len())
                    .for_each(|k, builder| {
                        let mat_opening = builder.get(&opened_values, k);
                        let mat = builder.get(&mats, k);
                        let mat_points = mat.points;
                        let mat_values = mat.values;
                        let domain = mat.domain;
                        let log2_domain_size = domain.log_n;
                        let log_height =
                            builder.eval_expr(log2_domain_size + RVar::from(log_blowup));

                        let cur_ro = builder.get(&ro, log_height);
                        let cur_alpha_pow = builder.get(&alpha_pow, log_height);

                        let bits_reduced: Usize<_> =
                            builder.eval(log_global_max_height - log_height);
                        let index_bits_shifted = index_bits.shift(builder, bits_reduced);

                        let two_adic_generator = config.get_two_adic_generator(builder, log_height);
                        builder.cycle_tracker_start("exp-reverse-bits-len");
                        let two_adic_generator_exp = builder.exp_reverse_bits_len(
                            two_adic_generator,
                            &index_bits_shifted,
                            log_height,
                        );
                        builder.cycle_tracker_end("exp-reverse-bits-len");
                        let x: Felt<C::F> = builder.eval(two_adic_generator_exp * g);

                        builder.range(0, mat_points.len()).for_each(|l, builder| {
                            let z: Ext<C::F, C::EF> = builder.get(&mat_points, l);
                            let ps_at_z = builder.get(&mat_values, l);

                            builder.cycle_tracker_start("sp1-fri-fold");
                            builder.range(0, ps_at_z.len()).for_each(|t, builder| {
                                let p_at_x = builder.get(&mat_opening, t);
                                let p_at_z = builder.get(&ps_at_z, t);
                                let quotient = (p_at_z - p_at_x) / (z - x);

                                builder.assign(&cur_ro, cur_ro + cur_alpha_pow * quotient);
                                builder.assign(&cur_alpha_pow, cur_alpha_pow * alpha);
                            });
                            builder.cycle_tracker_end("sp1-fri-fold");
                        });

                        builder.set_value(&ro, log_height, cur_ro);
                        builder.set_value(&alpha_pow, log_height, cur_alpha_pow);
                    });
                builder.cycle_tracker_end("compute-reduced-opening");
            });

            builder.set_value(&reduced_openings, i, ro);
        });
    builder.cycle_tracker_end("stage-d-2-fri-fold");

    builder.cycle_tracker_start("stage-d-3-verify-challenges");
    verify_challenges(
        builder,
        config,
        &fri_proof,
        &fri_challenges,
        &reduced_openings,
    );
    builder.cycle_tracker_end("stage-d-3-verify-challenges");
}

impl<C: Config> FromConstant<C> for TwoAdicPcsRoundVariable<C>
where
    C::F: TwoAdicField,
{
    type Constant = (
        Hash<C::F, C::F, DIGEST_SIZE>,
        Vec<(TwoAdicMultiplicativeCoset<C::F>, Vec<(C::EF, Vec<C::EF>)>)>,
    );

    fn constant(value: Self::Constant, builder: &mut Builder<C>) -> Self {
        let (commit_val, domains_and_openings_val) = value;

        // Allocate the commitment.
        let commit = builder.dyn_array::<Felt<_>>(DIGEST_SIZE);
        let commit_val: [C::F; DIGEST_SIZE] = commit_val.into();
        for (i, f) in commit_val.into_iter().enumerate() {
            builder.set(&commit, i, f);
        }

        let mats = builder
            .dyn_array::<TwoAdicPcsMatsVariable<C>>(RVar::from(domains_and_openings_val.len()));

        for (i, (domain, openning)) in domains_and_openings_val.into_iter().enumerate() {
            let domain = builder.constant::<TwoAdicMultiplicativeCosetVariable<_>>(domain);

            let points_val = openning.iter().map(|(p, _)| *p).collect::<Vec<_>>();
            let values_val = openning.iter().map(|(_, v)| v.clone()).collect::<Vec<_>>();
            let points: Array<_, Ext<_, _>> = builder.dyn_array(points_val.len());
            for (j, point) in points_val.into_iter().enumerate() {
                let el: Ext<_, _> = builder.eval(point.cons());
                builder.set_value(&points, j, el);
            }
            let values: Array<_, Array<_, Ext<_, _>>> = builder.dyn_array(values_val.len());
            for (j, val) in values_val.into_iter().enumerate() {
                let tmp = builder.dyn_array(val.len());
                for (k, v) in val.into_iter().enumerate() {
                    let el: Ext<_, _> = builder.eval(v.cons());
                    builder.set_value(&tmp, k, el);
                }
                builder.set_value(&values, j, tmp);
            }
            let mat = TwoAdicPcsMatsVariable {
                domain,
                points,
                values,
            };
            builder.set_value(&mats, i, mat);
        }

        Self {
            batch_commit: DigestVariable::Felt(commit),
            mats,
            permutation: builder.dyn_array(0),
        }
    }
}

#[derive(Clone)]
pub struct TwoAdicFriPcsVariable<C: Config> {
    pub config: FriConfigVariable<C>,
}

impl<C: Config> PcsVariable<C> for TwoAdicFriPcsVariable<C>
where
    C::F: TwoAdicField,
    C::EF: TwoAdicField,
{
    type Domain = TwoAdicMultiplicativeCosetVariable<C>;

    type Commitment = DigestVariable<C>;

    type Proof = TwoAdicPcsProofVariable<C>;

    fn natural_domain_for_log_degree(
        &self,
        builder: &mut Builder<C>,
        log_degree: RVar<C::N>,
    ) -> Self::Domain {
        self.config.get_subgroup(builder, log_degree)
    }

    // Todo: change TwoAdicPcsRoundVariable to RoundVariable
    fn verify(
        &self,
        builder: &mut Builder<C>,
        rounds: Array<C, TwoAdicPcsRoundVariable<C>>,
        proof: Self::Proof,
        challenger: &mut impl ChallengerVariable<C>,
    ) {
        verify_two_adic_pcs(builder, &self.config, rounds, proof, challenger)
    }
}

pub mod tests {
    use std::cmp::Reverse;

    use afs_compiler::{
        asm::AsmBuilder,
        ir::{Array, RVar, DIGEST_SIZE},
    };
    use ax_sdk::config::baby_bear_poseidon2::{default_engine, BabyBearPoseidon2Config};
    use itertools::Itertools;
    use p3_baby_bear::BabyBear;
    use p3_challenger::{CanObserve, FieldChallenger};
    use p3_commit::{Pcs, TwoAdicMultiplicativeCoset};
    use p3_matrix::dense::RowMajorMatrix;
    use p3_uni_stark::{StarkGenericConfig, Val};
    use rand::rngs::OsRng;
    use stark_vm::system::program::Program;

    use crate::{
        challenger::{duplex::DuplexChallengerVariable, CanObserveDigest, FeltChallenger},
        commit::PcsVariable,
        digest::DigestVariable,
        fri::{
            types::TwoAdicPcsRoundVariable, TwoAdicFriPcsVariable,
            TwoAdicMultiplicativeCosetVariable,
        },
        hints::{Hintable, InnerPcsProof, InnerVal},
        utils::const_fri_config,
    };

    #[allow(dead_code)]
    pub fn build_test_fri_with_cols_and_log2_rows(
        nb_cols: usize,
        nb_log2_rows: usize,
    ) -> (Program<BabyBear>, Vec<Vec<BabyBear>>) {
        type SC = BabyBearPoseidon2Config;
        type F = Val<SC>;
        type EF = <SC as StarkGenericConfig>::Challenge;
        type Challenger = <SC as StarkGenericConfig>::Challenger;
        type ScPcs = <SC as StarkGenericConfig>::Pcs;

        let mut rng = &mut OsRng;
        let log_degrees = &[nb_log2_rows];
        let engine = default_engine(27);
        let pcs = engine.config.pcs();
        let perm = engine.perm;

        // Generate proof.
        let domains_and_polys = log_degrees
            .iter()
            .map(|&d| {
                (
                    <ScPcs as Pcs<EF, Challenger>>::natural_domain_for_degree(pcs, 1 << d),
                    RowMajorMatrix::<F>::rand(&mut rng, 1 << d, nb_cols),
                )
            })
            .sorted_by_key(|(dom, _)| Reverse(dom.log_n))
            .collect::<Vec<_>>();
        let (commit, data) = <ScPcs as Pcs<EF, Challenger>>::commit(pcs, domains_and_polys.clone());
        let mut challenger = Challenger::new(perm.clone());
        challenger.observe(commit);
        let zeta = challenger.sample_ext_element::<EF>();
        let points = domains_and_polys
            .iter()
            .map(|_| vec![zeta])
            .collect::<Vec<_>>();
        let (opening, proof) = pcs.open(vec![(&data, points)], &mut challenger);

        // Verify proof.
        let mut challenger = Challenger::new(perm.clone());
        challenger.observe(commit);
        challenger.sample_ext_element::<EF>();
        let os: Vec<(TwoAdicMultiplicativeCoset<F>, Vec<_>)> = domains_and_polys
            .iter()
            .zip(&opening[0])
            .map(|((domain, _), mat_openings)| (*domain, vec![(zeta, mat_openings[0].clone())]))
            .collect();
        pcs.verify(vec![(commit, os.clone())], &proof, &mut challenger)
            .unwrap();

        // Test the recursive Pcs.
        let mut builder = AsmBuilder::<F, EF>::default();
        let config = const_fri_config(&mut builder, &engine.fri_params);
        let pcs_var = TwoAdicFriPcsVariable { config };
        let rounds =
            builder.constant::<Array<_, TwoAdicPcsRoundVariable<_>>>(vec![(commit, os.clone())]);

        // Test natural domain for degree.
        for log_d_val in log_degrees.iter() {
            let log_d = *log_d_val;
            let domain = pcs_var.natural_domain_for_log_degree(&mut builder, RVar::from(log_d));

            let domain_val =
                <ScPcs as Pcs<EF, Challenger>>::natural_domain_for_degree(pcs, 1 << log_d_val);

            let expected_domain: TwoAdicMultiplicativeCosetVariable<_> =
                builder.constant(domain_val);

            builder.assert_eq::<TwoAdicMultiplicativeCosetVariable<_>>(domain, expected_domain);
        }

        // Test proof verification.
        let proofvar = InnerPcsProof::read(&mut builder);
        let mut challenger = DuplexChallengerVariable::new(&mut builder);
        let commit = <[InnerVal; DIGEST_SIZE]>::from(commit).to_vec();
        let commit = DigestVariable::Felt(builder.constant::<Array<_, _>>(commit));
        challenger.observe_digest(&mut builder, commit);
        challenger.sample_ext(&mut builder);
        pcs_var.verify(&mut builder, rounds, proofvar, &mut challenger);
        builder.halt();

        let program = builder.compile_isa();
        let mut witness_stream = Vec::new();
        witness_stream.extend(proof.write());
        (program, witness_stream)
    }

    #[test]
    fn test_two_adic_fri_pcs_single_batch() {
        let (program, witness) = build_test_fri_with_cols_and_log2_rows(10, 10);
        stark_vm::system::program::util::execute_program(program, witness);
    }
}
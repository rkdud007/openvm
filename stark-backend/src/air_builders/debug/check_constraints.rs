use p3_air::BaseAir;
use p3_field::AbstractField;
use p3_matrix::dense::RowMajorMatrixView;
use p3_matrix::stack::VerticalPair;
use p3_matrix::Matrix;
use p3_maybe_rayon::prelude::*;
use p3_uni_stark::{StarkGenericConfig, Val};

use crate::air_builders::debug::DebugConstraintBuilder;
use crate::rap::Rap;

/// Check that all constraints vanish on the subgroup.
pub fn check_constraints<R, SC>(
    rap: &R,
    preprocessed: &Option<RowMajorMatrixView<Val<SC>>>,
    partitioned_main: &[RowMajorMatrixView<Val<SC>>],
    after_challenge: &[RowMajorMatrixView<SC::Challenge>],
    challenges: &[Vec<SC::Challenge>],
    public_values: &[Val<SC>],
    exposed_values_after_challenge: &[Vec<SC::Challenge>],
) where
    R: for<'a> Rap<DebugConstraintBuilder<'a, SC>> + BaseAir<Val<SC>> + ?Sized,
    SC: StarkGenericConfig,
{
    let height = partitioned_main[0].height();
    assert!(partitioned_main.iter().all(|mat| mat.height() == height));
    assert!(after_challenge.iter().all(|mat| mat.height() == height));

    // Check that constraints are satisfied.
    (0..height).into_par_iter().for_each(|i| {
        let i_next = (i + 1) % height;

        let (preprocessed_local, preprocessed_next) = preprocessed
            .as_ref()
            .map(|preprocessed| {
                (
                    preprocessed.row_slice(i).to_vec(),
                    preprocessed.row_slice(i_next).to_vec(),
                )
            })
            .unwrap_or((vec![], vec![]));

        let partitioned_main_row_pair = partitioned_main
            .iter()
            .map(|part| (part.row_slice(i), part.row_slice(i_next)))
            .collect::<Vec<_>>();
        let partitioned_main = partitioned_main_row_pair
            .iter()
            .map(|(local, next)| {
                VerticalPair::new(
                    RowMajorMatrixView::new_row(local),
                    RowMajorMatrixView::new_row(next),
                )
            })
            .collect::<Vec<_>>();

        let after_challenge_row_pair = after_challenge
            .iter()
            .map(|mat| (mat.row_slice(i), mat.row_slice(i_next)))
            .collect::<Vec<_>>();
        let after_challenge = after_challenge_row_pair
            .iter()
            .map(|(local, next)| {
                VerticalPair::new(
                    RowMajorMatrixView::new_row(local),
                    RowMajorMatrixView::new_row(next),
                )
            })
            .collect::<Vec<_>>();

        let mut builder = DebugConstraintBuilder {
            row_index: i,
            preprocessed: VerticalPair::new(
                RowMajorMatrixView::new_row(preprocessed_local.as_slice()),
                RowMajorMatrixView::new_row(preprocessed_next.as_slice()),
            ),
            partitioned_main,
            after_challenge,
            challenges,
            public_values,
            exposed_values_after_challenge,
            is_first_row: Val::<SC>::zero(),
            is_last_row: Val::<SC>::zero(),
            is_transition: Val::<SC>::one(),
        };
        if i == 0 {
            builder.is_first_row = Val::<SC>::one();
        }
        if i == height - 1 {
            builder.is_last_row = Val::<SC>::one();
            builder.is_transition = Val::<SC>::zero();
        }

        rap.eval(&mut builder);
    });
}
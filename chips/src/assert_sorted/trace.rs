use p3_field::PrimeField64;
use p3_matrix::dense::RowMajorMatrix;

use crate::sub_chip::LocalTraceInstructions;

use super::{columns::AssertSortedCols, AssertSortedChip};

impl AssertSortedChip {
    pub fn generate_trace<F: PrimeField64>(&self) -> RowMajorMatrix<F> {
        let num_cols: usize = AssertSortedCols::<F>::get_width(
            self.air.is_less_than_tuple_air().limb_bits().clone(),
            *self.air.is_less_than_tuple_air().decomp(),
            self.air.is_less_than_tuple_air().tuple_len(),
        );

        let mut rows: Vec<F> = vec![];
        for i in 0..self.air.keys().len() {
            let key = self.air.keys()[i].clone();
            let next_key: Vec<u32> = if i == self.air.keys().len() - 1 {
                vec![0; self.air.is_less_than_tuple_air().tuple_len()]
            } else {
                self.air.keys()[i + 1].clone()
            };

            let is_less_than_tuple_trace = LocalTraceInstructions::generate_trace_row(
                self.air.is_less_than_tuple_air(),
                (key.clone(), next_key.clone(), self.range_checker.clone()),
            )
            .flatten();

            // the current key
            let mut row: Vec<F> =
                is_less_than_tuple_trace[0..self.air.is_less_than_tuple_air().tuple_len()].to_vec();
            // the less than indicator and the auxiliary columns
            row.extend_from_slice(
                &is_less_than_tuple_trace[2 * self.air.is_less_than_tuple_air().tuple_len()..],
            );

            rows.extend_from_slice(&row);
        }

        RowMajorMatrix::new(rows, num_cols)
    }
}
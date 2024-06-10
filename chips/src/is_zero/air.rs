use std::borrow::Borrow;

use afs_stark_backend::interaction::AirBridge;
use p3_air::{Air, AirBuilder, BaseAir};
use p3_field::AbstractField;
use p3_field::Field;
use p3_matrix::Matrix;

use crate::sub_chip::{AirConfig, SubAir};

use super::{
    columns::{IsZeroCols, IsZeroIOCols, NUM_COLS},
    IsZeroAir,
};

impl<F: Field> BaseAir<F> for IsZeroAir {
    fn width(&self) -> usize {
        NUM_COLS
    }
}

impl AirConfig for IsZeroAir {
    type Cols<T> = IsZeroCols<T>;
}

// No interactions
impl<F: Field> AirBridge<F> for IsZeroAir {}

impl<AB: AirBuilder> Air<AB> for IsZeroAir {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();

        let local = main.row_slice(0);
        let is_zero_cols: &IsZeroCols<_> = (*local).borrow();

        SubAir::<AB>::eval(self, builder, is_zero_cols.io, is_zero_cols.inv);
    }
}

impl<AB: AirBuilder> SubAir<AB> for IsZeroAir {
    type IoView = IsZeroIOCols<AB::Var>;
    type AuxView = AB::Var;

    fn eval(&self, builder: &mut AB, io: Self::IoView, inv: Self::AuxView) {
        builder.assert_eq(io.x * io.is_zero, AB::F::zero());
        builder.assert_eq(io.is_zero + io.x * inv, AB::F::one());
    }
}
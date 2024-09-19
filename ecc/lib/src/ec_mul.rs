use afs_compiler::ir::{Array, BigUintVar, Builder, Config, RVar, Var};
use p3_field::{AbstractField, PrimeField64};

use crate::types::ECPointVariable;

pub const BIGINT_MAX_BITS: usize = 256;

pub fn scalar_multiply_secp256k1<C>(
    builder: &mut Builder<C>,
    point: &ECPointVariable<C>,
    scalar: BigUintVar<C>,
    window_bits: usize,
) -> ECPointVariable<C>
where
    C: Config,
    C::N: PrimeField64,
{
    assert_eq!(BIGINT_MAX_BITS % window_bits, 0);
    let ECPointVariable { x, y } = point;
    let num_windows = BIGINT_MAX_BITS / window_bits;
    let window_len = (1usize << window_bits) - 1;

    let x_zero = builder.secp256k1_coord_is_zero(x);
    let y_zero = builder.secp256k1_coord_is_zero(y);
    let result_x: BigUintVar<C> = builder.uninit_biguint();
    let result_y: BigUintVar<C> = builder.uninit_biguint();

    builder.secp256k1_coord_set_to_zero(&result_x);
    builder.secp256k1_coord_set_to_zero(&result_y);

    builder.if_eq(x_zero * y_zero, C::N::one()).then_or_else(
        |_builder| {},
        |builder| {
            let mut increment = point.clone();
            let cached_points_jacobian = (0..num_windows)
                .map(|_| {
                    let mut curr = increment.clone();
                    // start with increment at index 0 instead of identity just as a dummy value to avoid divide by 0 issues
                    let cache_vec: Array<C, ECPointVariable<C>> = builder.dyn_array(window_len);
                    for j in 0..window_len {
                        let prev = curr.clone();
                        let (curr_x, curr_y) = builder.ec_add(
                            &(curr.x, curr.y),
                            &(increment.x.clone(), increment.y.clone()),
                        );
                        curr = ECPointVariable {
                            x: curr_x,
                            y: curr_y,
                        };
                        builder.set(&cache_vec, j, prev.clone());
                    }
                    increment = curr;
                    cache_vec
                })
                .collect::<Vec<_>>();
            let bits = builder.num2bits_biguint(&scalar);
            for (i, cache_vec) in cached_points_jacobian.iter().enumerate() {
                let window_sum: Var<C::N> = builder.uninit();
                builder.assign(&window_sum, RVar::zero());
                for j in 0..window_bits {
                    let bit = builder.get(&bits, RVar::from(i * window_bits + window_bits - j - 1));
                    builder.assign(&window_sum, window_sum + window_sum + bit);
                }
                builder.if_ne(window_sum, C::N::zero()).then(|builder| {
                    builder.assign(&window_sum, window_sum - RVar::one());
                    let point = builder.get(cache_vec, window_sum);
                    let (x, y) =
                        builder.ec_add(&(result_x.clone(), result_y.clone()), &(point.x, point.y));
                    builder.assign(&result_x, x);
                    builder.assign(&result_y, y);
                });
            }
        },
    );
    ECPointVariable {
        x: result_x,
        y: result_y,
    }
}
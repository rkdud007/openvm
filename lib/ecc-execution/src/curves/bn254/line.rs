use halo2curves_axiom::ff::Field;

use crate::common::{EvaluatedLine, FieldExtension};

/// Multiplies two elements in 013 form and outputs the product in 01234 form
pub fn mul_013_by_013<Fp, Fp2>(
    line_0: EvaluatedLine<Fp, Fp2>,
    line_1: EvaluatedLine<Fp, Fp2>,
    // TODO[yj]: once this function is moved into a chip, we can use the xi property instead of passing in this argument
    xi: Fp2,
) -> [Fp2; 5]
where
    Fp: Field,
    Fp2: FieldExtension<BaseField = Fp>,
{
    let b0 = line_0.b;
    let c0 = line_0.c;
    let b1 = line_1.b;
    let c1 = line_1.c;

    // where w⁶ = xi
    // l0 * l1 = 1 + (b0 + b1)w + (b0b1)w² + (c0 + c1)w³ + (b0c1 + b1c0)w⁴ + (c0c1)w⁶
    //         = (1 + c0c1 * xi) + (b0 + b1)w + (b0b1)w² + (c0 + c1)w³ + (b0c1 + b1c0)w⁴
    let l0 = Fp2::ONE + c0 * c1 * xi;
    let l1 = b0 + b1;
    let l2 = b0 * b1;
    let l3 = c0 + c1;
    let l4 = b0 * c1 + b1 * c0;

    [l0, l1, l2, l3, l4]
}

pub fn mul_by_013<Fp, Fp2, Fp12>(f: Fp12, line: EvaluatedLine<Fp, Fp2>) -> Fp12
where
    Fp: Field,
    Fp2: FieldExtension<BaseField = Fp>,
    Fp12: FieldExtension<BaseField = Fp2>,
{
    mul_by_01234(f, [Fp2::ONE, line.b, Fp2::ZERO, line.c, Fp2::ZERO])
}

pub fn mul_by_01234<Fp, Fp2, Fp12>(f: Fp12, x: [Fp2; 5]) -> Fp12
where
    Fp: Field,
    Fp2: FieldExtension<BaseField = Fp>,
    Fp12: FieldExtension<BaseField = Fp2>,
{
    let x_fp12 = Fp12::from_coeffs(&x);
    f * x_fp12
}
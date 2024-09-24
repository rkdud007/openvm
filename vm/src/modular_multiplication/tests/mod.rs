use std::borrow::Cow;

use ax_sdk::{config::setup_tracing, utils::create_seeded_rng};
use num_bigint_dig::{algorithms::mod_inverse, BigUint};
use p3_baby_bear::BabyBear;
use rand::{Rng, RngCore};

use crate::{
    arch::{
        instructions::{
            Opcode::*, SECP256K1_COORD_MODULAR_ARITHMETIC_INSTRUCTIONS,
            SECP256K1_SCALAR_MODULAR_ARITHMETIC_INSTRUCTIONS,
        },
        testing::MachineChipTestBuilder,
    },
    program::Instruction,
    modular_multiplication::{
        biguint_to_elems, ModularArithmeticChip, NUM_ELEMS, REPR_BITS, SECP256K1_COORD_PRIME,
        SECP256K1_SCALAR_PRIME,
    },
};

#[test]
fn test_modular_multiplication_runtime() {
    setup_tracing();

    let mut tester: MachineChipTestBuilder<BabyBear> = MachineChipTestBuilder::default();
    let mut coord_chip = ModularArithmeticChip::new(
        tester.memory_chip(),
        SECP256K1_COORD_PRIME.clone(),
        REPR_BITS,
    );
    let mut scalar_chip = ModularArithmeticChip::new(
        tester.memory_chip(),
        SECP256K1_SCALAR_PRIME.clone(),
        REPR_BITS,
    );
    let mut rng = create_seeded_rng();
    for _ in 0..100 {
        let mut a_digits = [0; NUM_ELEMS];
        rng.fill_bytes(&mut a_digits);
        let mut a = BigUint::from_bytes_le(&a_digits);
        let mut b_digits = [0; NUM_ELEMS];
        rng.fill_bytes(&mut b_digits);
        let mut b = BigUint::from_bytes_le(&b_digits);
        // if these are not true then trace is not guaranteed to be verifiable
        let is_scalar = rng.gen_bool(0.5);
        let (modulus, opcode) = if is_scalar {
            (
                SECP256K1_SCALAR_PRIME.clone(),
                SECP256K1_SCALAR_MODULAR_ARITHMETIC_INSTRUCTIONS[rng.gen_range(0..4)],
            )
        } else {
            (
                SECP256K1_COORD_PRIME.clone(),
                SECP256K1_COORD_MODULAR_ARITHMETIC_INSTRUCTIONS[rng.gen_range(0..4)],
            )
        };

        a %= modulus.clone();
        b %= modulus.clone();
        assert!(a < modulus);
        assert!(b < modulus);

        let r = match opcode {
            SECP256K1_COORD_ADD | SECP256K1_SCALAR_ADD => a.clone() + b.clone(),
            SECP256K1_COORD_SUB | SECP256K1_SCALAR_SUB => a.clone() + modulus.clone() - b.clone(),
            SECP256K1_COORD_MUL | SECP256K1_SCALAR_MUL => a.clone() * b.clone(),
            SECP256K1_COORD_DIV | SECP256K1_SCALAR_DIV => {
                a.clone()
                    * mod_inverse(Cow::Borrowed(&b), Cow::Borrowed(&modulus))
                        .unwrap()
                        .to_biguint()
                        .unwrap()
            }

            _ => panic!(),
        } % modulus;
        let address1 = 0;
        let address2 = 100;
        let address3 = 4000;
        let num_elems = NUM_ELEMS;
        let repr_bits = REPR_BITS;

        for (i, &elem) in biguint_to_elems(a, repr_bits, num_elems).iter().enumerate() {
            let address = address1 + i;
            tester.write_cell(1, address, elem);
        }
        for (i, &elem) in biguint_to_elems(b, repr_bits, num_elems).iter().enumerate() {
            let address = address2 + i;
            tester.write_cell(1, address, elem);
        }
        let (raddress2, raddress3) = match opcode {
            SECP256K1_COORD_ADD | SECP256K1_SCALAR_ADD => (address2, address3),
            SECP256K1_COORD_SUB | SECP256K1_SCALAR_SUB => (address3, address2),
            SECP256K1_COORD_MUL | SECP256K1_SCALAR_MUL => (address2, address3),
            SECP256K1_COORD_DIV | SECP256K1_SCALAR_DIV => (address3, address2),
            _ => panic!(),
        };
        let instruction = Instruction::from_isize(
            opcode,
            address1 as isize,
            raddress2 as isize,
            raddress3 as isize,
            0,
            1,
        );
        if is_scalar {
            tester.execute(&mut scalar_chip, instruction);
        } else {
            tester.execute(&mut coord_chip, instruction);
        }
        for (i, &elem) in biguint_to_elems::<BabyBear>(r, repr_bits, num_elems)
            .iter()
            .enumerate()
        {
            let address = address3 + i;
            let read_val = tester.read_cell(1, address);
            assert_eq!(elem, read_val);
        }
    }
}
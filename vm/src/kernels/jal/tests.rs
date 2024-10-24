use std::borrow::BorrowMut;

use afs_stark_backend::{
    utils::disable_debug_builder, verifier::VerificationError, Chip, ChipUsageGetter,
};
use ax_sdk::utils::create_seeded_rng;
use p3_air::BaseAir;
use p3_baby_bear::BabyBear;
use p3_field::{AbstractField, PrimeField32};
use p3_matrix::{dense::RowMajorMatrix, Matrix};
use rand::{rngs::StdRng, Rng};

use super::JalCoreCols;
use crate::{
    arch::{
        instructions::{
            NativeJalOpcode::{self, *},
            UsizeOpcode,
        },
        testing::VmChipTestBuilder,
        VmAdapterChip,
    },
    kernels::{
        adapters::jal_native_adapter::JalNativeAdapterChip,
        jal::{JalCoreChip, KernelJalChip},
    },
    system::{program::Instruction, PC_BITS},
};

type F = BabyBear;

fn set_and_execute(
    tester: &mut VmChipTestBuilder<F>,
    chip: &mut KernelJalChip<F>,
    rng: &mut StdRng,
    initial_imm: Option<u32>,
    initial_pc: Option<u32>,
) {
    let imm = initial_imm.unwrap_or(rng.gen_range(0..20));
    let a = rng.gen_range(0..32) << 2;
    let d = rng.gen_range(1..4);

    tester.execute_with_pc(
        chip,
        Instruction::from_usize(
            JAL as usize + NativeJalOpcode::default_offset(),
            [a, imm as usize, 0, d, 0, 0, 0],
        ),
        initial_pc.unwrap_or(rng.gen_range(0..(1 << PC_BITS))),
    );
    let initial_pc = tester.execution.last_from_pc().as_canonical_u32();
    let final_pc = tester.execution.last_to_pc().as_canonical_u32();

    let next_pc = initial_pc + imm;
    let rd_data = initial_pc + 1;

    assert_eq!(next_pc, final_pc);
    assert_eq!(rd_data, tester.read::<1>(d, a)[0].as_canonical_u32());
}

fn setup() -> (
    StdRng,
    VmChipTestBuilder<F>,
    KernelJalChip<F>,
    JalNativeAdapterChip<F>,
) {
    let rng = create_seeded_rng();
    let tester = VmChipTestBuilder::default();

    let adapter = JalNativeAdapterChip::<F>::new(
        tester.execution_bus(),
        tester.program_bus(),
        tester.memory_controller(),
    );
    let inner = JalCoreChip::new(NativeJalOpcode::default_offset());
    let chip = KernelJalChip::<F>::new(adapter.clone(), inner, tester.memory_controller());
    (rng, tester, chip, adapter)
}

#[test]
fn rand_jal_test() {
    let (mut rng, mut tester, mut chip, _) = setup();
    let num_tests: usize = 100;
    for _ in 0..num_tests {
        set_and_execute(&mut tester, &mut chip, &mut rng, None, None);
    }

    let tester = tester.build().load(chip).finalize();
    tester.simple_test().expect("Verification failed");
}

#[test]
fn negative_jal_test() {
    let (mut rng, mut tester, mut chip, adapter) = setup();
    set_and_execute(&mut tester, &mut chip, &mut rng, None, None);

    let jal_trace_width = chip.trace_width();
    let mut chip_input = chip.generate_air_proof_input();
    let jal_trace = chip_input.raw.common_main.as_mut().unwrap();
    let adapter_width = BaseAir::<F>::width(adapter.air());
    {
        let mut trace_row = jal_trace.row_slice(0).to_vec();
        let (_, core_row) = trace_row.split_at_mut(adapter_width);
        let core_cols: &mut JalCoreCols<F> = core_row.borrow_mut();
        core_cols.imm = F::from_canonical_u32(rng.gen_range(1 << 11..1 << 12));
        *jal_trace = RowMajorMatrix::new(trace_row, jal_trace_width);
    }
    disable_debug_builder();
    let tester = tester.build().load_air_proof_input(chip_input).finalize();
    let msg = format!(
        "Expected verification to fail with {:?}, but it didn't",
        VerificationError::NonZeroCumulativeSum
    );
    let result = tester.simple_test();
    assert_eq!(
        result.err(),
        Some(VerificationError::NonZeroCumulativeSum),
        "{}",
        msg
    );
}
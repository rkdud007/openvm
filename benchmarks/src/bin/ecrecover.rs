#![allow(unused_variables)]
#![allow(unused_imports)]

use clap::Parser;
use derive_more::derive::From;
use eyre::Result;
use k256::ecdsa::{RecoveryId, Signature, SigningKey, VerifyingKey};
use num_bigint_dig::BigUint;
use openvm_algebra_circuit::{
    ModularExtension, ModularExtensionExecutor, ModularExtensionPeriphery, Rv32ModularConfig,
    Rv32ModularWithFp2Config,
};
use openvm_algebra_transpiler::ModularTranspilerExtension;
use openvm_benchmarks::utils::{bench_from_exe, build_bench_program, BenchmarkCli};
use openvm_circuit::{
    arch::{
        instructions::exe::VmExe, SystemConfig, SystemExecutor, SystemPeriphery, VmChipComplex,
        VmConfig, VmInventoryError,
    },
    derive::{AnyEnum, InstructionExecutor, VmConfig},
};
use openvm_circuit_primitives_derive::{Chip, ChipUsageGetter};
use openvm_ecc_circuit::{
    CurveConfig, Rv32WeierstrassConfig, WeierstrassExtension, WeierstrassExtensionExecutor,
    WeierstrassExtensionPeriphery, SECP256K1_CONFIG,
};
use openvm_ecc_transpiler::EccTranspilerExtension;
use openvm_keccak256_circuit::{Keccak256, Keccak256Executor, Keccak256Periphery};
use openvm_keccak256_transpiler::Keccak256TranspilerExtension;
use openvm_native_compiler::conversion::CompilerOptions;
use openvm_native_recursion::testing_utils::inner::build_verification_program;
use openvm_rv32im_circuit::{
    Rv32I, Rv32IExecutor, Rv32IPeriphery, Rv32Io, Rv32IoExecutor, Rv32IoPeriphery, Rv32M,
    Rv32MExecutor, Rv32MPeriphery,
};
use openvm_rv32im_transpiler::{
    Rv32ITranspilerExtension, Rv32IoTranspilerExtension, Rv32MTranspilerExtension,
};
use openvm_sdk::{config::AppConfig, StdIn};
use openvm_stark_backend::p3_field::{AbstractField, PrimeField32};
use openvm_stark_sdk::{
    bench::run_with_metric_collection,
    config::{baby_bear_poseidon2::BabyBearPoseidon2Engine, FriParameters},
    engine::StarkFriEngine,
    p3_baby_bear::BabyBear,
    p3_keccak::Keccak256Hash,
};
use openvm_transpiler::{transpiler::Transpiler, FromElf};
use rand_chacha::{rand_core::SeedableRng, ChaCha8Rng};
use serde::{Deserialize, Serialize};
use tiny_keccak::{Hasher, Keccak};
use tracing::info_span;

fn make_input(signing_key: &SigningKey, msg: &[u8]) -> Vec<BabyBear> {
    let mut hasher = Keccak::v256();
    hasher.update(msg);
    let mut prehash = [0u8; 32];
    hasher.finalize(&mut prehash);
    let (signature, recid) = signing_key.sign_prehash_recoverable(&prehash).unwrap();
    // Input format: https://www.evm.codes/precompiled?fork=cancun#0x01
    let mut input = prehash.to_vec();
    let v = recid.to_byte() + 27u8;
    input.extend_from_slice(&[0; 31]);
    input.push(v);
    input.extend_from_slice(signature.to_bytes().as_ref());

    input.into_iter().map(BabyBear::from_canonical_u8).collect()
}

#[derive(Clone, Debug, VmConfig, derive_new::new, Serialize, Deserialize)]
pub struct Rv32ImEcRecoverConfig {
    #[system]
    pub system: SystemConfig,
    #[extension]
    pub base: Rv32I,
    #[extension]
    pub mul: Rv32M,
    #[extension]
    pub io: Rv32Io,
    #[extension]
    pub modular: ModularExtension,
    #[extension]
    pub keccak: Keccak256,
    #[extension]
    pub weierstrass: WeierstrassExtension,
}

impl Rv32ImEcRecoverConfig {
    pub fn for_curves(curves: Vec<CurveConfig>) -> Self {
        let primes: Vec<BigUint> = curves
            .iter()
            .flat_map(|c| [c.modulus.clone(), c.scalar.clone()])
            .collect();
        Self {
            system: SystemConfig::default().with_continuations(),
            base: Default::default(),
            mul: Default::default(),
            io: Default::default(),
            modular: ModularExtension::new(primes),
            keccak: Default::default(),
            weierstrass: WeierstrassExtension::new(curves),
        }
    }
}

fn main() -> Result<()> {
    let cli_args = BenchmarkCli::parse();
    let app_log_blowup = cli_args.app_log_blowup.unwrap_or(2);
    let agg_log_blowup = cli_args.agg_log_blowup.unwrap_or(2);

    let elf = build_bench_program("ecrecover")?;
    let exe = VmExe::from_elf(
        elf,
        Transpiler::<BabyBear>::default()
            .with_extension(Rv32ITranspilerExtension)
            .with_extension(Rv32MTranspilerExtension)
            .with_extension(Rv32IoTranspilerExtension)
            .with_extension(Keccak256TranspilerExtension)
            .with_extension(ModularTranspilerExtension)
            .with_extension(EccTranspilerExtension),
    )?;
    // TODO: update sw_setup macros and read it from elf.
    let vm_config = AppConfig {
        app_fri_params: FriParameters::standard_with_100_bits_conjectured_security(app_log_blowup)
            .into(),
        app_vm_config: Rv32ImEcRecoverConfig::for_curves(vec![SECP256K1_CONFIG.clone()]),
        leaf_fri_params: FriParameters::standard_with_100_bits_conjectured_security(agg_log_blowup)
            .into(),
        compiler_options: CompilerOptions::default().with_cycle_tracker(),
    };

    run_with_metric_collection("OUTPUT_PATH", || -> Result<()> {
        info_span!("ECDSA Recover Program").in_scope(|| {
            let mut rng = ChaCha8Rng::seed_from_u64(12345);
            let signing_key: SigningKey = SigningKey::random(&mut rng);
            let verifying_key = VerifyingKey::from(&signing_key);
            let mut hasher = Keccak::v256();
            let mut expected_address = [0u8; 32];
            hasher.update(
                &verifying_key
                    .to_encoded_point(/* compress = */ false)
                    .as_bytes()[1..],
            );
            hasher.finalize(&mut expected_address);
            expected_address[..12].fill(0); // 20 bytes as the address.
            let mut input_stream = vec![expected_address
                .into_iter()
                .map(BabyBear::from_canonical_u8)
                .collect::<Vec<_>>()];

            let msg = ["Elliptic", "Curve", "Digital", "Signature", "Algorithm"];
            input_stream.extend(
                msg.iter()
                    .map(|s| make_input(&signing_key, s.as_bytes()))
                    .collect::<Vec<_>>(),
            );
            bench_from_exe(
                "ecrecover_program",
                vm_config,
                exe,
                input_stream.into(),
                false,
            )
        })?;

        Ok(())
    })
}

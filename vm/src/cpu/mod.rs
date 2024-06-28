//use crate::range_gate::RangeCheckerGateChip;

use enum_utils::FromStr;

#[cfg(test)]
pub mod tests;

pub mod air;
pub mod bridge;
pub mod columns;
pub mod trace;

pub const WORD_SIZE: usize = 1;
pub const INST_WIDTH: usize = 1;

pub const READ_INSTRUCTION_BUS: usize = 0;
pub const MEMORY_BUS: usize = 1;
pub const ARITHMETIC_BUS: usize = 2;
pub const RANGE_CHECKER_BUS: usize = 3;

pub const MAX_READS_PER_CYCLE: usize = 2;
pub const MAX_WRITES_PER_CYCLE: usize = 1;

#[derive(Copy, Clone, Debug, PartialEq, Eq, FromStr, PartialOrd, Ord)]
#[repr(usize)]
pub enum OpCode {
    LOADW = 0,
    STOREW = 1,
    JAL = 2,
    BEQ = 3,
    BNE = 4,
    TERMINATE = 5,

    FADD = 6,
    FSUB = 7,
    FMUL = 8,
    FDIV = 9,

    FAIL = 10,
    PRINTF = 11,
}

impl OpCode {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(OpCode::LOADW),
            1 => Some(OpCode::STOREW),
            2 => Some(OpCode::JAL),
            3 => Some(OpCode::BEQ),
            4 => Some(OpCode::BNE),
            5 => Some(OpCode::TERMINATE),
            6 => Some(OpCode::FADD),
            7 => Some(OpCode::FSUB),
            8 => Some(OpCode::FMUL),
            9 => Some(OpCode::FDIV),
            10 => Some(OpCode::FAIL),
            11 => Some(OpCode::PRINTF),
            _ => None,
        }
    }
}

use OpCode::*;

const CORE_INSTRUCTIONS: [OpCode; 6] = [LOADW, STOREW, JAL, BEQ, BNE, TERMINATE];
const FIELD_ARITHMETIC_INSTRUCTIONS: [OpCode; 4] = [FADD, FSUB, FMUL, FDIV];

#[derive(Default, Clone, Copy)]
pub struct CpuOptions {
    pub field_arithmetic_enabled: bool,
}

impl CpuOptions {
    pub fn enabled_instructions(&self) -> Vec<OpCode> {
        let mut result = CORE_INSTRUCTIONS.to_vec();
        if self.field_arithmetic_enabled {
            result.extend(FIELD_ARITHMETIC_INSTRUCTIONS);
        }
        result
    }

    pub fn num_enabled_instructions(&self) -> usize {
        self.enabled_instructions().len()
    }
}

#[derive(Default, Clone)]
pub struct CpuAir {
    pub options: CpuOptions,
}

impl CpuAir {
    pub fn new(options: CpuOptions) -> Self {
        Self { options }
    }
}
//! Portable stack bytecode and its mandatory pre-execution verifier.

use std::{collections::VecDeque, fmt};

use crate::runtime::JsString;

mod compiler;
mod vm;
pub use compiler::{BytecodeCompiler, CompileError};
pub use vm::VirtualMachine;

/// Pointer-free literal stored in a bytecode constant pool.
#[derive(Clone, Debug, PartialEq)]
pub enum Constant {
    /// ECMAScript `undefined`.
    Undefined,
    /// ECMAScript `null`.
    Null,
    /// Boolean literal.
    Boolean(bool),
    /// IEEE-754 binary64 number.
    Number(f64),
    /// Exact UTF-16 string, including lone surrogates.
    String(JsString),
}

/// Portable instruction with fixed-width integer operands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Instruction {
    /// Push constant-pool entry `index` (`+1`).
    LoadConstant(u32),
    /// Push local slot `index` (`+1`).
    LoadLocal(u32),
    /// Pop and write local slot `index` (`-1`).
    StoreLocal(u32),
    /// Duplicate the stack top (`+1`, requires one value).
    Duplicate,
    /// Discard the stack top (`-1`).
    Pop,
    /// Pop two operands and push their numeric sum (`-1`).
    Add,
    /// Pop two operands and push their numeric difference (`-1`).
    Subtract,
    /// Pop two operands and push their numeric product (`-1`).
    Multiply,
    /// Pop two operands and push their numeric quotient (`-1`).
    Divide,
    /// Pop two operands and push strict equality (`-1`).
    StrictEqual,
    /// Replace the stack top with its logical negation (`0`).
    LogicalNot,
    /// Continue at absolute instruction `target` (`0`).
    Jump(u32),
    /// Pop a condition and jump when false (`-1`).
    JumpIfFalse(u32),
    /// Pop and produce a function-return completion (`-1`, terminal).
    Return,
    /// Pop and produce a normal program completion (`-1`, terminal).
    Complete,
    /// Pop and produce a throw completion (`-1`, terminal).
    Throw,
}

impl Instruction {
    fn required_stack(self) -> usize {
        match self {
            Self::LoadConstant(_) | Self::LoadLocal(_) | Self::Jump(_) => 0,
            Self::StoreLocal(_)
            | Self::Duplicate
            | Self::Pop
            | Self::JumpIfFalse(_)
            | Self::LogicalNot
            | Self::Return
            | Self::Complete
            | Self::Throw => 1,
            Self::Add | Self::Subtract | Self::Multiply | Self::Divide | Self::StrictEqual => 2,
        }
    }

    fn stack_delta(self) -> isize {
        match self {
            Self::LoadConstant(_) | Self::LoadLocal(_) | Self::Duplicate => 1,
            Self::StoreLocal(_)
            | Self::Pop
            | Self::JumpIfFalse(_)
            | Self::Return
            | Self::Complete
            | Self::Throw => -1,
            Self::Add | Self::Subtract | Self::Multiply | Self::Divide | Self::StrictEqual => -1,
            Self::Jump(_) | Self::LogicalNot => 0,
        }
    }
}

/// Untrusted portable bytecode plus its constant pool and local-slot count.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bytecode {
    /// Pointer-free literals referenced by `LoadConstant`.
    pub constants: Vec<Constant>,
    /// Instructions addressed by zero-based absolute indices.
    pub instructions: Vec<Instruction>,
    /// Number of local slots available to load and store instructions.
    pub local_count: u32,
}

impl Bytecode {
    /// Validates operands, stack discipline, joins, reachability, and exits.
    pub fn verify(self) -> Result<VerifiedBytecode, VerifyError> {
        verify(self)
    }
}

/// Bytecode proven safe for VM dispatch.
#[derive(Clone, Debug, PartialEq)]
pub struct VerifiedBytecode {
    bytecode: Bytecode,
    max_stack: usize,
}

impl VerifiedBytecode {
    /// Returns the verified bytecode body.
    #[must_use]
    pub fn bytecode(&self) -> &Bytecode {
        &self.bytecode
    }

    /// Returns the maximum operand-stack depth required by reachable code.
    #[must_use]
    pub fn max_stack(&self) -> usize {
        self.max_stack
    }

    /// Consumes the proof wrapper and returns the bytecode body.
    #[must_use]
    pub fn into_bytecode(self) -> Bytecode {
        self.bytecode
    }
}

/// Stable verifier failure category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerifyErrorKind {
    /// No instruction exists at the entry point.
    Empty,
    /// A constant-pool operand is outside the pool.
    ConstantOutOfBounds,
    /// A local-slot operand is outside the declared frame.
    LocalOutOfBounds,
    /// A branch target is not an instruction boundary.
    JumpOutOfBounds,
    /// An instruction would pop more values than are available.
    StackUnderflow,
    /// Two control-flow edges reach an instruction at different depths.
    StackDepthMismatch,
    /// Execution can fall beyond the final instruction.
    MissingReturn,
    /// A return does not consume the only remaining stack value.
    InvalidReturnDepth,
    /// An instruction is not reachable from entry.
    UnreachableInstruction,
}

/// Verifier diagnostic carrying an instruction offset when available.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifyError {
    kind: VerifyErrorKind,
    offset: Option<usize>,
}

impl VerifyError {
    /// Returns the stable failure category.
    #[must_use]
    pub fn kind(&self) -> VerifyErrorKind {
        self.kind
    }

    /// Returns the offending instruction index, if one exists.
    #[must_use]
    pub fn offset(&self) -> Option<usize> {
        self.offset
    }

    fn at(kind: VerifyErrorKind, offset: usize) -> Self {
        Self {
            kind,
            offset: Some(offset),
        }
    }
}

impl fmt::Display for VerifyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.offset {
            Some(offset) => write!(
                formatter,
                "bytecode verification failed at {offset}: {:?}",
                self.kind
            ),
            None => write!(formatter, "bytecode verification failed: {:?}", self.kind),
        }
    }
}

impl std::error::Error for VerifyError {}

fn verify(bytecode: Bytecode) -> Result<VerifiedBytecode, VerifyError> {
    let length = bytecode.instructions.len();
    if length == 0 {
        return Err(VerifyError {
            kind: VerifyErrorKind::Empty,
            offset: None,
        });
    }
    let mut depths = vec![None; length];
    let mut work = VecDeque::from([(0_usize, 0_usize)]);
    let mut max_stack = 0;
    while let Some((offset, depth)) = work.pop_front() {
        if let Some(previous) = depths[offset] {
            if previous != depth {
                return Err(VerifyError::at(VerifyErrorKind::StackDepthMismatch, offset));
            }
            continue;
        }
        depths[offset] = Some(depth);
        let instruction = bytecode.instructions[offset];
        match instruction {
            Instruction::LoadConstant(index) if index as usize >= bytecode.constants.len() => {
                return Err(VerifyError::at(
                    VerifyErrorKind::ConstantOutOfBounds,
                    offset,
                ));
            }
            Instruction::LoadLocal(index) | Instruction::StoreLocal(index)
                if index >= bytecode.local_count =>
            {
                return Err(VerifyError::at(VerifyErrorKind::LocalOutOfBounds, offset));
            }
            Instruction::Jump(target) | Instruction::JumpIfFalse(target)
                if target as usize >= length =>
            {
                return Err(VerifyError::at(VerifyErrorKind::JumpOutOfBounds, offset));
            }
            _ => {}
        }
        if depth < instruction.required_stack() {
            return Err(VerifyError::at(VerifyErrorKind::StackUnderflow, offset));
        }
        if matches!(
            instruction,
            Instruction::Return | Instruction::Complete | Instruction::Throw
        ) && depth != 1
        {
            return Err(VerifyError::at(VerifyErrorKind::InvalidReturnDepth, offset));
        }
        let next_depth = depth
            .checked_add_signed(instruction.stack_delta())
            .expect("stack requirement prevents underflow");
        max_stack = max_stack.max(depth).max(next_depth);
        match instruction {
            Instruction::Return | Instruction::Complete | Instruction::Throw => {}
            Instruction::Jump(target) => work.push_back((target as usize, next_depth)),
            Instruction::JumpIfFalse(target) => {
                work.push_back((target as usize, next_depth));
                enqueue_fallthrough(offset, length, next_depth, &mut work)?;
            }
            _ => enqueue_fallthrough(offset, length, next_depth, &mut work)?,
        }
    }
    if let Some(offset) = depths.iter().position(Option::is_none) {
        return Err(VerifyError::at(
            VerifyErrorKind::UnreachableInstruction,
            offset,
        ));
    }
    Ok(VerifiedBytecode {
        bytecode,
        max_stack,
    })
}

fn enqueue_fallthrough(
    offset: usize,
    length: usize,
    depth: usize,
    work: &mut VecDeque<(usize, usize)>,
) -> Result<(), VerifyError> {
    let next = offset + 1;
    if next == length {
        Err(VerifyError::at(VerifyErrorKind::MissingReturn, offset))
    } else {
        work.push_back((next, depth));
        Ok(())
    }
}

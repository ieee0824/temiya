//! Feature-gated experimental baseline machine-code backend.
//!
//! This module exists only with the `jit` Cargo feature. The default engine
//! and bytecode interpreter have no executable-memory or JIT dependency.

use std::{fmt, marker::PhantomData, rc::Rc};

use crate::bytecode::VerifiedBytecode;

#[cfg(all(target_arch = "x86_64", unix))]
use crate::bytecode::{Constant, Instruction};

/// Stable baseline-JIT failure category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JitErrorKind {
    /// The current CPU/OS backend is not implemented.
    UnsupportedPlatform,
    /// The verified instruction is outside the experimental baseline subset.
    UnsupportedInstruction,
    /// The program does not finish with a normal numeric completion.
    UnsupportedCompletion,
    /// More virtual values are live than the baseline register allocator supports.
    RegisterPressure,
    /// Executable-memory allocation or protection failed.
    MemoryProtection,
}

/// Error returned while lowering or installing baseline machine code.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JitError {
    kind: JitErrorKind,
    offset: Option<usize>,
}

impl JitError {
    /// Returns the stable error category.
    #[must_use]
    pub fn kind(&self) -> JitErrorKind {
        self.kind
    }

    /// Returns the unsupported bytecode instruction index, if applicable.
    #[must_use]
    pub fn offset(&self) -> Option<usize> {
        self.offset
    }

    fn new(kind: JitErrorKind, offset: Option<usize>) -> Self {
        Self { kind, offset }
    }
}

impl fmt::Display for JitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.offset {
            Some(offset) => write!(
                formatter,
                "baseline JIT failed at {offset}: {:?}",
                self.kind
            ),
            None => write!(formatter, "baseline JIT failed: {:?}", self.kind),
        }
    }
}

impl std::error::Error for JitError {}

/// Compiler for the feature-gated baseline machine-code subset.
#[derive(Clone, Copy, Debug, Default)]
pub struct BaselineJit;

impl BaselineJit {
    /// Lowers verified bytecode into an owned executable code object.
    ///
    /// The initial backend supports straight-line numeric constants and the
    /// four arithmetic instructions. Unsupported input is rejected without
    /// changing the interpreter fallback contract.
    pub fn compile(bytecode: &VerifiedBytecode) -> Result<JitCode, JitError> {
        platform::compile(bytecode)
    }
}

/// Owned executable baseline code.
///
/// The mapping is thread-confined and released on drop. Callers retain the
/// verified bytecode separately if they want an interpreter fallback.
pub struct JitCode {
    allocation: platform::ExecutableMemory,
    thread_confined: PhantomData<Rc<()>>,
}

impl JitCode {
    /// Executes machine code and returns its binary64 result.
    #[must_use]
    pub fn execute(&self) -> f64 {
        // SAFETY: `compile` installs a backend-generated function with this ABI,
        // and the executable mapping remains owned for the duration of the call.
        unsafe { self.allocation.call() }
    }

    /// Returns the number of emitted machine-code bytes.
    #[must_use]
    pub fn code_size(&self) -> usize {
        self.allocation.len()
    }
}

impl fmt::Debug for JitCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("JitCode")
            .field("code_size", &self.code_size())
            .finish_non_exhaustive()
    }
}

#[cfg(all(target_arch = "x86_64", unix))]
mod platform {
    use std::{ffi::c_void, ptr};

    use super::*;

    const REGISTER_LIMIT: usize = 8;

    pub(super) fn compile(bytecode: &VerifiedBytecode) -> Result<JitCode, JitError> {
        let body = bytecode.bytecode();
        let mut machine = Vec::new();
        let mut depth = 0_usize;
        let mut passthrough_load = None;
        for (offset, instruction) in body.instructions.iter().copied().enumerate() {
            match instruction {
                Instruction::EnterScope(index) if matches!(&body.constants[index as usize], Constant::Scope(plan) if plan.kind == crate::bytecode::ScopeKind::Global && plan.bindings.is_empty()) =>
                    {}
                Instruction::ExitScope => {}
                Instruction::LoadConstant(index) => {
                    let Constant::Number(number) = body.constants[index as usize] else {
                        return Err(JitError::new(
                            JitErrorKind::UnsupportedInstruction,
                            Some(offset),
                        ));
                    };
                    if depth == REGISTER_LIMIT {
                        return Err(JitError::new(JitErrorKind::RegisterPressure, Some(offset)));
                    }
                    emit_number(&mut machine, depth as u8, number);
                    depth += 1;
                }
                Instruction::Add
                | Instruction::Subtract
                | Instruction::Multiply
                | Instruction::Divide => {
                    let right = depth - 1;
                    let left = depth - 2;
                    emit_binary(&mut machine, instruction, left as u8, right as u8);
                    depth -= 1;
                }
                Instruction::StoreLocal(index)
                    if body.instructions.get(offset + 1)
                        == Some(&Instruction::LoadLocal(index)) =>
                {
                    passthrough_load = Some(offset + 1);
                }
                Instruction::LoadLocal(_) if passthrough_load == Some(offset) => {
                    passthrough_load = None;
                }
                Instruction::Complete if depth == 1 => {
                    if depth - 1 != 0 {
                        emit_move(&mut machine, 0, (depth - 1) as u8);
                    }
                    machine.push(0xc3);
                }
                Instruction::Complete => {
                    return Err(JitError::new(
                        JitErrorKind::UnsupportedCompletion,
                        Some(offset),
                    ));
                }
                _ => {
                    return Err(JitError::new(
                        JitErrorKind::UnsupportedInstruction,
                        Some(offset),
                    ));
                }
            }
        }
        if machine.last() != Some(&0xc3) {
            return Err(JitError::new(JitErrorKind::UnsupportedCompletion, None));
        }
        Ok(JitCode {
            allocation: ExecutableMemory::new(&machine)?,
            thread_confined: PhantomData,
        })
    }

    fn emit_number(code: &mut Vec<u8>, register: u8, number: f64) {
        code.extend_from_slice(&[0x48, 0xb8]);
        code.extend_from_slice(&number.to_bits().to_le_bytes());
        code.extend_from_slice(&[0x66, 0x48, 0x0f, 0x6e, 0xc0 | (register << 3)]);
    }

    fn emit_binary(code: &mut Vec<u8>, instruction: Instruction, left: u8, right: u8) {
        let opcode = match instruction {
            Instruction::Add => 0x58,
            Instruction::Subtract => 0x5c,
            Instruction::Multiply => 0x59,
            Instruction::Divide => 0x5e,
            _ => unreachable!("only numeric binary instructions are emitted"),
        };
        code.extend_from_slice(&[0xf2, 0x0f, opcode, 0xc0 | (left << 3) | right]);
    }

    fn emit_move(code: &mut Vec<u8>, destination: u8, source: u8) {
        code.extend_from_slice(&[0x66, 0x0f, 0x28, 0xc0 | (destination << 3) | source]);
    }

    pub(super) struct ExecutableMemory {
        pointer: *mut c_void,
        length: usize,
    }

    impl ExecutableMemory {
        fn new(code: &[u8]) -> Result<Self, JitError> {
            // SAFETY: Anonymous private mapping with a nonzero byte length.
            let pointer = unsafe {
                libc::mmap(
                    ptr::null_mut(),
                    code.len(),
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                    -1,
                    0,
                )
            };
            if pointer == libc::MAP_FAILED {
                return Err(JitError::new(JitErrorKind::MemoryProtection, None));
            }
            // SAFETY: The mapping is writable and at least `code.len()` bytes.
            unsafe { ptr::copy_nonoverlapping(code.as_ptr(), pointer.cast(), code.len()) };
            // SAFETY: The same live mapping is transitioned from RW to RX.
            if unsafe { libc::mprotect(pointer, code.len(), libc::PROT_READ | libc::PROT_EXEC) }
                != 0
            {
                // SAFETY: This is the mapping just allocated above.
                unsafe { libc::munmap(pointer, code.len()) };
                return Err(JitError::new(JitErrorKind::MemoryProtection, None));
            }
            Ok(Self {
                pointer,
                length: code.len(),
            })
        }

        pub(super) unsafe fn call(&self) -> f64 {
            // SAFETY: The mapping begins with a generated `extern "C" fn() -> f64`.
            let function: unsafe extern "C" fn() -> f64 =
                unsafe { std::mem::transmute(self.pointer) };
            // SAFETY: Guaranteed by the generated code and live mapping invariant.
            unsafe { function() }
        }

        pub(super) fn len(&self) -> usize {
            self.length
        }
    }

    impl Drop for ExecutableMemory {
        fn drop(&mut self) {
            // SAFETY: This object uniquely owns the live mapping.
            unsafe { libc::munmap(self.pointer, self.length) };
        }
    }
}

#[cfg(not(all(target_arch = "x86_64", unix)))]
mod platform {
    use super::*;

    pub(super) fn compile(_: &VerifiedBytecode) -> Result<JitCode, JitError> {
        Err(JitError::new(JitErrorKind::UnsupportedPlatform, None))
    }

    pub(super) struct ExecutableMemory;
    impl ExecutableMemory {
        pub(super) unsafe fn call(&self) -> f64 {
            unreachable!()
        }
        pub(super) fn len(&self) -> usize {
            0
        }
    }
}

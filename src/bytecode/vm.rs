use crate::runtime::{Completion, RuntimeError, RuntimeResult, Value, binary_values};

use super::{Constant, Instruction, VerifiedBytecode};

/// Baseline stack VM dispatching only verifier-approved bytecode.
#[derive(Clone, Copy, Debug, Default)]
pub struct VirtualMachine;

impl VirtualMachine {
    /// Executes verified bytecode while preserving normal, return, and throw completions.
    pub fn execute(bytecode: &VerifiedBytecode) -> RuntimeResult<Completion> {
        let body = bytecode.bytecode();
        let mut stack = Vec::with_capacity(bytecode.max_stack());
        let mut locals = vec![Value::Undefined; body.local_count as usize];
        let mut pc = 0;
        loop {
            let instruction = body.instructions[pc];
            pc += 1;
            match instruction {
                Instruction::LoadConstant(index) => {
                    stack.push(constant(&body.constants[index as usize]))
                }
                Instruction::LoadLocal(index) => stack.push(locals[index as usize].clone()),
                Instruction::StoreLocal(index) => locals[index as usize] = pop(&mut stack)?,
                Instruction::Duplicate => stack.push(
                    stack
                        .last()
                        .cloned()
                        .ok_or_else(|| RuntimeError::internal_error("verified stack was empty"))?,
                ),
                Instruction::Pop => {
                    pop(&mut stack)?;
                }
                Instruction::Add => binary(&mut stack, "+")?,
                Instruction::Subtract => binary(&mut stack, "-")?,
                Instruction::Multiply => binary(&mut stack, "*")?,
                Instruction::Divide => binary(&mut stack, "/")?,
                Instruction::StrictEqual => binary(&mut stack, "===")?,
                Instruction::LogicalNot => {
                    let value = pop(&mut stack)?;
                    stack.push(Value::Boolean(!value.to_boolean()));
                }
                Instruction::Jump(target) => pc = target as usize,
                Instruction::JumpIfFalse(target) => {
                    if !pop(&mut stack)?.to_boolean() {
                        pc = target as usize;
                    }
                }
                Instruction::Return => return Ok(Completion::Return(pop(&mut stack)?)),
                Instruction::Complete => return Ok(Completion::Normal(Some(pop(&mut stack)?))),
                Instruction::Throw => return Ok(Completion::Throw(pop(&mut stack)?)),
            }
        }
    }
}

fn constant(constant: &Constant) -> Value {
    match constant {
        Constant::Undefined => Value::Undefined,
        Constant::Null => Value::Null,
        Constant::Boolean(value) => Value::Boolean(*value),
        Constant::Number(value) => Value::Number(*value),
        Constant::String(value) => Value::String(value.clone()),
    }
}

fn pop(stack: &mut Vec<Value>) -> RuntimeResult<Value> {
    stack
        .pop()
        .ok_or_else(|| RuntimeError::internal_error("verified stack was empty"))
}

fn binary(stack: &mut Vec<Value>, operator: &str) -> RuntimeResult<()> {
    let right = pop(stack)?;
    let left = pop(stack)?;
    stack.push(binary_values(operator, left, right)?);
    Ok(())
}

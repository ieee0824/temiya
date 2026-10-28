//! A guarded leaf execution path derived only from verifier-approved bytecode.
use super::{
    BinaryOperator, BindingKind, Bytecode, Constant, Instruction, ScopeKind, UnaryNumericOperator,
};
use crate::runtime::{JsString, Value, binary_number, unary_numeric_value};

const STACK_LIMIT: usize = 8;

#[derive(Clone, Debug, PartialEq)]
enum Step {
    Number(f64),
    Read { name: JsString, own: bool },
    Binary(BinaryOperator),
    Unary(UnaryNumericOperator),
    Pop,
    Duplicate,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct NumericReturnPlan(Vec<Step>);

impl NumericReturnPlan {
    pub(super) fn compile(code: &Bytecode, max_stack: usize) -> Option<Self> {
        if max_stack > STACK_LIMIT {
            return None;
        }
        let Instruction::EnterScope(index) = *code.instructions.first()? else {
            return None;
        };
        let Constant::Scope(scope) = code.constants.get(index as usize)? else {
            return None;
        };
        if scope.kind != ScopeKind::Function
            || !scope.functions.is_empty()
            || scope
                .bindings
                .iter()
                .any(|binding| binding.kind != BindingKind::Var)
        {
            return None;
        }
        let mut steps = Vec::new();
        let mut depth = 0_usize;
        for instruction in &code.instructions[1..] {
            let step = match *instruction {
                Instruction::LoadConstant(index) => {
                    let Constant::Number(number) = code.constants.get(index as usize)? else {
                        return None;
                    };
                    depth += 1;
                    Step::Number(*number)
                }
                Instruction::LoadBinding { scope: 0, slot } => {
                    depth += 1;
                    Step::Read {
                        name: scope.bindings.get(slot as usize)?.name.clone(),
                        own: true,
                    }
                }
                Instruction::GetName { name, .. } => {
                    let Constant::String(name) = code.constants.get(name as usize)? else {
                        return None;
                    };
                    depth += 1;
                    Step::Read {
                        name: name.clone(),
                        own: scope.bindings.iter().any(|binding| binding.name == *name),
                    }
                }
                Instruction::Add => {
                    depth = depth.checked_sub(1)?;
                    Step::Binary(BinaryOperator::Add)
                }
                Instruction::Subtract => {
                    depth = depth.checked_sub(1)?;
                    Step::Binary(BinaryOperator::Subtract)
                }
                Instruction::Multiply => {
                    depth = depth.checked_sub(1)?;
                    Step::Binary(BinaryOperator::Multiply)
                }
                Instruction::Divide => {
                    depth = depth.checked_sub(1)?;
                    Step::Binary(BinaryOperator::Divide)
                }
                Instruction::StrictEqual => {
                    depth = depth.checked_sub(1)?;
                    Step::Binary(BinaryOperator::StrictEqual)
                }
                Instruction::Binary(operator) => {
                    depth = depth.checked_sub(1)?;
                    Step::Binary(operator)
                }
                Instruction::UnaryNumeric(operator) => Step::Unary(operator),
                Instruction::Pop => {
                    depth = depth.checked_sub(1)?;
                    Step::Pop
                }
                Instruction::Duplicate => {
                    depth += 1;
                    Step::Duplicate
                }
                Instruction::Return if depth == 1 => return Some(Self(steps)),
                _ => return None,
            };
            steps.push(step);
        }
        None
    }

    pub(super) fn execute(
        &self,
        parameters: &[JsString],
        arguments: &[Value],
        mut captures: impl FnMut(&JsString) -> Option<f64>,
    ) -> Option<Value> {
        let mut stack: [Value; STACK_LIMIT] = std::array::from_fn(|_| Value::Undefined);
        let mut depth = 0;
        for step in &self.0 {
            match step {
                Step::Number(number) => {
                    stack[depth] = Value::Number(*number);
                    depth += 1;
                }
                Step::Read { name, own } => {
                    let number = if let Some(index) =
                        parameters.iter().rposition(|parameter| parameter == name)
                    {
                        let Value::Number(number) = arguments.get(index)? else {
                            return None;
                        };
                        *number
                    } else if *own {
                        return None;
                    } else {
                        captures(name)?
                    };
                    stack[depth] = Value::Number(number);
                    depth += 1;
                }
                Step::Binary(operator) => {
                    let right = std::mem::replace(&mut stack[depth - 1], Value::Undefined);
                    depth -= 1;
                    let Value::Number(right) = right else {
                        return None;
                    };
                    let Value::Number(left) = stack[depth - 1] else {
                        return None;
                    };
                    stack[depth - 1] = binary_number(*operator, left, right)?;
                }
                Step::Unary(operator) => {
                    let Value::Number(number) = stack[depth - 1] else {
                        return None;
                    };
                    stack[depth - 1] =
                        unary_numeric_value(None, operator.spelling(), Value::Number(number))
                            .ok()?;
                }
                Step::Pop => {
                    depth -= 1;
                    stack[depth] = Value::Undefined;
                }
                Step::Duplicate => {
                    stack[depth] = stack[depth - 1].clone();
                    depth += 1;
                }
            }
        }
        Some(std::mem::replace(&mut stack[0], Value::Undefined))
    }
}

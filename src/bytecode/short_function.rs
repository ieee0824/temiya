//! Bounded execution for already-verified, unobservable empty function frames.
use super::{BinaryOperator, Bytecode, Constant, Instruction, VerifiedBytecode};
use crate::runtime::{
    Completion, Environment, JsString, Realm, Reference, RuntimeError, RuntimeResult, Value,
    binary_number, binary_values_in_realm,
};

#[cfg(test)]
thread_local! {
    static SHORT_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub(super) fn proven(code: &Bytecode, max_stack: usize, has_backedge: bool) -> bool {
    if max_stack > 4 || code.local_count != 0 || has_backedge || code.instructions.len() > 64 {
        return false;
    }
    let mut reference = false;
    for instruction in &code.instructions {
        match instruction {
            Instruction::ResolveName { .. } => {
                if reference {
                    return false;
                }
                reference = true;
            }
            Instruction::LoadReference => {
                if !reference {
                    return false;
                }
            }
            Instruction::StoreReference
            | Instruction::DiscardReference
            | Instruction::UpdateReference { .. }
            | Instruction::TypeOfReference => {
                if !reference {
                    return false;
                }
                reference = false;
            }
            // No pending reference crosses a branch; the normal verifier has
            // already proved operand depths and every target's validity.
            Instruction::Jump(_)
            | Instruction::JumpIfFalse(_)
            | Instruction::Return
            | Instruction::Complete
            | Instruction::Throw => {
                if reference {
                    return false;
                }
            }
            Instruction::EnterScope(_)
            | Instruction::ExitScope
            | Instruction::LoadName(_)
            | Instruction::GetName { .. }
            | Instruction::LoadConstant(_)
            | Instruction::Duplicate
            | Instruction::Pop
            | Instruction::Binary(_)
            | Instruction::Add
            | Instruction::Subtract
            | Instruction::Multiply
            | Instruction::Divide
            | Instruction::StrictEqual
            | Instruction::LogicalNot
            | Instruction::UnaryNumeric(_)
            | Instruction::TypeOf
            | Instruction::CreateObjectLiteral(_) => {}
            _ => return false,
        }
    }
    !reference
}

struct Values {
    slots: [Value; 4],
    len: usize,
}
impl Values {
    fn push(&mut self, value: Value) -> RuntimeResult<()> {
        let slot = self
            .slots
            .get_mut(self.len)
            .ok_or_else(|| RuntimeError::internal_error("short function stack overflow"))?;
        *slot = value;
        self.len += 1;
        Ok(())
    }
    fn pop(&mut self) -> RuntimeResult<Value> {
        self.len = self
            .len
            .checked_sub(1)
            .ok_or_else(|| RuntimeError::internal_error("short function stack underflow"))?;
        Ok(std::mem::replace(
            &mut self.slots[self.len],
            Value::Undefined,
        ))
    }
}

pub(super) fn execute(
    code: &VerifiedBytecode,
    realm: &Realm,
    outer: &Environment,
) -> RuntimeResult<Completion> {
    #[cfg(test)]
    SHORT_CALLS.with(|calls| calls.set(calls.get() + 1));
    let _roots = realm.enter_object_root_region();
    let body = code.bytecode();
    let mut definitions = code.definition_context();
    let mut values = Values {
        slots: [const { Value::Undefined }; 4],
        len: 0,
    };
    let mut reference: Option<Reference> = None;
    let mut pc = 0;
    let name = |index: u32| -> &JsString {
        let Constant::String(name) = &body.constants[index as usize] else {
            unreachable!()
        };
        name
    };
    loop {
        let instruction = body.instructions[pc];
        pc += 1;
        match instruction {
            // Every scope is proven empty; no activation/loop cells are reused.
            Instruction::EnterScope(_) | Instruction::ExitScope => {}
            Instruction::LoadConstant(index) => {
                values.push(super::vm::constant(&body.constants[index as usize]))?
            }
            Instruction::LoadName(index) => {
                values.push(outer.get_identifier_value(name(index), true)?)?
            }
            Instruction::GetName {
                name: index,
                strict,
            } => values.push(outer.get_identifier_value(name(index), strict)?)?,
            Instruction::ResolveName {
                name: index,
                strict,
            } => {
                let name = name(index);
                reference = Some(if let Some(binding) = outer.resolve_binding(name)? {
                    Reference::environment(binding, name.clone(), strict)
                } else {
                    Reference::unresolvable(realm.global_object(), name.clone(), strict)
                });
            }
            Instruction::LoadReference => values.push(
                reference
                    .as_ref()
                    .expect("verified reference")
                    .get_value()?,
            )?,
            Instruction::DiscardReference => {
                reference.take();
            }
            Instruction::StoreReference => reference
                .take()
                .expect("verified reference")
                .put_value(values.pop()?)?,
            Instruction::UpdateReference { increment, prefix } => {
                let target = reference.take().expect("verified reference");
                values.push(crate::runtime::update_reference_in_realm(
                    realm, &target, increment, prefix,
                )?)?;
            }
            Instruction::TypeOfReference => {
                let target = reference.take().expect("verified reference");
                let value = if target.is_unresolvable() {
                    Value::Undefined
                } else {
                    target.get_value()?
                };
                values.push(Value::String(crate::runtime::type_of(&value).into()))?;
            }
            Instruction::Duplicate => {
                let value = values.pop()?;
                values.push(value.clone())?;
                values.push(value)?;
            }
            Instruction::Pop => {
                values.pop()?;
            }
            Instruction::LogicalNot => {
                let value = values.pop()?;
                values.push(Value::Boolean(!value.to_boolean()))?;
            }
            Instruction::TypeOf => {
                let value = values.pop()?;
                values.push(Value::String(crate::runtime::type_of(&value).into()))?;
            }
            Instruction::UnaryNumeric(operator) => {
                let value = values.pop()?;
                values.push(crate::runtime::unary_numeric_value(
                    Some(realm),
                    operator.spelling(),
                    value,
                )?)?;
            }
            Instruction::Binary(_)
            | Instruction::Add
            | Instruction::Subtract
            | Instruction::Multiply
            | Instruction::Divide
            | Instruction::StrictEqual => {
                let operator = match instruction {
                    Instruction::Binary(operator) => operator,
                    Instruction::Add => BinaryOperator::Add,
                    Instruction::Subtract => BinaryOperator::Subtract,
                    Instruction::Multiply => BinaryOperator::Multiply,
                    Instruction::Divide => BinaryOperator::Divide,
                    Instruction::StrictEqual => BinaryOperator::StrictEqual,
                    _ => unreachable!(),
                };
                let right = values.pop()?;
                let left = values.pop()?;
                let value = if let (Value::Number(a), Value::Number(b)) = (&left, &right) {
                    binary_number(operator, *a, *b).map(Ok).unwrap_or_else(|| {
                        binary_values_in_realm(realm, operator.spelling(), left, right)
                    })?
                } else {
                    binary_values_in_realm(realm, operator.spelling(), left, right)?
                };
                values.push(value)?;
            }
            Instruction::CreateObjectLiteral(index) => {
                let Constant::ObjectLiteral { definition, strict } =
                    &body.constants[index as usize]
                else {
                    unreachable!()
                };
                values.push(
                    definitions.evaluate_object_literal(definition, *strict, realm, outer)?,
                )?;
            }
            Instruction::Jump(target) => pc = target as usize,
            Instruction::JumpIfFalse(target) => {
                if !values.pop()?.to_boolean() {
                    pc = target as usize;
                }
            }
            Instruction::Return => return Ok(Completion::Return(values.pop()?)),
            Instruction::Complete => return Ok(Completion::Normal(Some(values.pop()?))),
            Instruction::Throw => return Ok(Completion::Throw(values.pop()?)),
            _ => {
                return Err(RuntimeError::internal_error(
                    "unproven short function instruction",
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{ast::StmtKind, bytecode::BytecodeCompiler, parser::parse_script};

    fn eligible(source: &str) -> bool {
        let program = parse_script(source).unwrap();
        let StmtKind::FunctionDeclaration(function) = &program.body[0].kind else {
            panic!("function");
        };
        BytecodeCompiler::compile_function_template(function)
            .unwrap()
            .body
            .has_short_function_path()
    }

    #[test]
    fn bounded_captured_reads_writes_and_metadata_are_eligible() {
        assert!(eligible(
            "function f(){if(state!==0)throw 42;state=1;reads++;return current>n;}"
        ));
        assert!(eligible(
            "function f(){return {next(){return 0;},return:function(){throw 99;}};}"
        ));
    }

    #[test]
    fn observers_loops_locals_and_nested_pending_references_use_general_vm() {
        for source in [
            "function f(){return this;}",
            "function f(){return arguments;}",
            "function f(){return new.target;}",
            "function f(){let x=1;return x;}",
            "function f(){while(x)x--;return x;}",
            "function f(){a=(b=1);return a;}",
            "function f(){a=x?1:2;return a;}",
            "function f(){return callback();}",
        ] {
            assert!(!eligible(source), "{source}");
        }
    }

    #[test]
    fn public_auto_executes_both_mutating_callbacks_and_metadata_factories_here() {
        use crate::{Engine, embedding::ScriptBackend, runtime::Value};
        let engine = Engine::builder()
            .script_backend(ScriptBackend::Auto)
            .build();
        let realm = engine.create_realm();
        let before = super::SHORT_CALLS.with(|calls| calls.get());
        let script=engine.compile(&realm,"let state=0,reads=0,current=2,n=1;function mark(){if(state!==0)throw 42;state=1;reads++;return current>n;}function factory(){return {next(){return 0;}};}let result=mark();let object=factory();result&&reads===1&&state===1&&object.next()===0;").unwrap();
        assert!(matches!(
            engine.evaluate(&script).unwrap().value(),
            Value::Boolean(true)
        ));
        assert!(super::SHORT_CALLS.with(|calls| calls.get()) >= before + 2);
    }
}

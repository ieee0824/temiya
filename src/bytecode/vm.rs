use crate::runtime::{
    Completion, Environment, EnvironmentKind, Realm, Reference, ReferencedName, RuntimeError,
    RuntimeResult, Value, binary_values, binary_values_in_realm,
};

use super::{BindingKind, Constant, Instruction, ScopeDescriptor, ScopeKind, VerifiedBytecode};

/// Baseline stack VM dispatching only verifier-approved bytecode.
#[derive(Clone, Copy, Debug, Default)]
pub struct VirtualMachine;

impl VirtualMachine {
    /// Executes verified bytecode while preserving normal, return, and throw completions.
    pub fn execute(bytecode: &VerifiedBytecode) -> RuntimeResult<Completion> {
        if bytecode
            .bytecode()
            .instructions
            .iter()
            .any(|op| matches!(op, Instruction::EnterScope(_)))
        {
            return Self::execute_in_realm(bytecode, &Realm::ordinary());
        }
        Self::run(bytecode, None, None)
    }

    /// Executes Script bytecode in a persistent host-supplied Realm.
    pub fn execute_in_realm(
        bytecode: &VerifiedBytecode,
        realm: &Realm,
    ) -> RuntimeResult<Completion> {
        Self::run(bytecode, Some(realm), None)
    }

    /// Executes function-body bytecode with a caller-prepared function frame.
    /// The caller owns parameter, arguments, this and capture instantiation.
    pub fn execute_in_function(
        bytecode: &VerifiedBytecode,
        realm: &Realm,
        frame: &Environment,
    ) -> RuntimeResult<Completion> {
        if frame.kind() != EnvironmentKind::Function {
            return Err(RuntimeError::type_error(
                "bytecode requires a function environment",
            ));
        }
        Self::run(bytecode, Some(realm), Some(frame))
    }

    fn run(
        bytecode: &VerifiedBytecode,
        realm: Option<&Realm>,
        function: Option<&Environment>,
    ) -> RuntimeResult<Completion> {
        let body = bytecode.bytecode();
        let mut scopes: Vec<ScopeFrame<'_>> = vec![];
        let mut references: Vec<Reference> = vec![];
        let mut calls: Vec<Invocation> = vec![];
        let mut stack = Vec::with_capacity(bytecode.max_stack());
        let mut locals = vec![Value::Undefined; body.local_count as usize];
        let mut pc = 0;
        loop {
            let instruction = body.instructions[pc];
            pc += 1;
            match instruction {
                Instruction::EnterScope(index) => {
                    let Constant::Scope(plan) = &body.constants[index as usize] else {
                        unreachable!()
                    };
                    let environment =
                        enter_scope(plan, scopes.last().map(|s| &s.environment), realm, function)?;
                    scopes.push(ScopeFrame {
                        entry: (pc - 1) as u32,
                        plan,
                        environment,
                    });
                }
                Instruction::ExitScope => {
                    scopes.pop();
                }
                Instruction::LoadBinding { scope, slot } => {
                    let frame = binding_frame(&scopes, scope);
                    stack.push(frame.environment.get_binding_value(
                        &frame.plan.bindings[slot as usize].name,
                        frame.plan.strict,
                    )?);
                }
                Instruction::InitializeBinding { scope, slot } => {
                    let frame = binding_frame(&scopes, scope);
                    frame.environment.initialize_binding(
                        &frame.plan.bindings[slot as usize].name,
                        pop(&mut stack)?,
                    )?;
                }
                Instruction::StoreBinding { scope, slot } => {
                    let frame = binding_frame(&scopes, scope);
                    frame.environment.set_mutable_binding(
                        &frame.plan.bindings[slot as usize].name,
                        pop(&mut stack)?,
                        frame.plan.strict,
                    )?;
                }
                Instruction::LoadName(index) => {
                    let Constant::String(name) = &body.constants[index as usize] else {
                        unreachable!()
                    };
                    stack.push(
                        scopes
                            .last()
                            .expect("verified active scope")
                            .environment
                            .get_identifier_value(name, true)?,
                    );
                }
                Instruction::ResolveName {
                    name: index,
                    strict,
                } => {
                    let Constant::String(name) = &body.constants[index as usize] else {
                        unreachable!()
                    };
                    let environment = &scopes.last().expect("verified active scope").environment;
                    let reference = if let Some(binding) = environment.resolve_binding(name)? {
                        Reference::environment(binding, name.clone(), strict)
                    } else {
                        Reference::unresolvable(
                            realm.expect("scope requires Realm").global_object(),
                            name.clone(),
                            strict,
                        )
                    };
                    references.push(reference);
                }
                Instruction::StoreReference => {
                    references
                        .pop()
                        .expect("verified reference stack")
                        .put_value(pop(&mut stack)?)?;
                }
                Instruction::GetProperty { strict } | Instruction::ResolveProperty { strict } => {
                    let key = pop(&mut stack)?;
                    let base = pop(&mut stack)?;
                    let reference = Reference::property_in_realm(
                        realm.expect("verified scoped property access"),
                        base,
                        ReferencedName::Value(key),
                        strict,
                    );
                    if matches!(instruction, Instruction::GetProperty { .. }) {
                        stack.push(reference.get_value()?);
                    } else {
                        references.push(reference);
                    }
                }
                Instruction::DeleteReference => {
                    stack.push(Value::Boolean(
                        references
                            .pop()
                            .expect("verified reference stack")
                            .delete()?,
                    ));
                }
                Instruction::BeginCall | Instruction::BeginConstruct => {
                    calls.push(Invocation {
                        callee: pop(&mut stack)?,
                        receiver: Value::Undefined,
                        construct: matches!(instruction, Instruction::BeginConstruct),
                        arguments: vec![],
                    });
                }
                Instruction::BeginReferenceCall => {
                    let reference = references.pop().expect("verified reference stack");
                    let receiver = reference.this_value().unwrap_or(Value::Undefined);
                    calls.push(Invocation {
                        callee: reference.get_value()?,
                        receiver,
                        construct: false,
                        arguments: vec![],
                    });
                }
                Instruction::AppendArgument => {
                    calls
                        .last_mut()
                        .expect("verified call frame")
                        .arguments
                        .push(pop(&mut stack)?);
                }
                Instruction::Invoke => {
                    let invocation = calls.pop().expect("verified call frame");
                    stack.push(invocation.invoke()?);
                }
                Instruction::LoadThis => {
                    let mut environment = scopes
                        .last()
                        .expect("verified active scope")
                        .environment
                        .clone();
                    loop {
                        if environment.kind() == EnvironmentKind::Global {
                            stack.push(environment.global_this_value()?);
                            break;
                        }
                        if environment.has_this_binding()? {
                            stack.push(environment.get_this_binding()?);
                            break;
                        }
                        environment = environment
                            .outer()
                            .ok_or_else(|| RuntimeError::reference_error("no this binding"))?;
                    }
                }
                Instruction::JumpIfNullish(target) => {
                    if matches!(pop(&mut stack)?, Value::Null | Value::Undefined) {
                        pc = target as usize;
                    }
                }
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
                Instruction::Add => binary(&mut stack, "+", realm)?,
                Instruction::Subtract => binary(&mut stack, "-", realm)?,
                Instruction::Multiply => binary(&mut stack, "*", realm)?,
                Instruction::Divide => binary(&mut stack, "/", realm)?,
                Instruction::StrictEqual => binary(&mut stack, "===", realm)?,
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
        Constant::Scope(_) => unreachable!("verifier rejects metadata values"),
        Constant::BigInt(value) => Value::BigInt(value.clone()),
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

fn binary(stack: &mut Vec<Value>, operator: &str, realm: Option<&Realm>) -> RuntimeResult<()> {
    let right = pop(stack)?;
    let left = pop(stack)?;
    stack.push(if let Some(realm) = realm {
        binary_values_in_realm(realm, operator, left, right)?
    } else {
        binary_values(operator, left, right)?
    });
    Ok(())
}

// Each invocation owns its roots independently of operand-stack growth and of
// nested/reentrant VM runs. All pending roots drop on an abrupt runtime result.
struct Invocation {
    callee: Value,
    receiver: Value,
    construct: bool,
    arguments: Vec<Value>,
}

impl Invocation {
    fn invoke(self) -> RuntimeResult<Value> {
        let Value::Object(callee) = self.callee else {
            return Err(RuntimeError::type_error(if self.construct {
                "value is not a constructor"
            } else {
                "value is not callable"
            }));
        };
        if self.construct {
            Ok(Value::Object(
                callee.construct(&self.arguments, callee.clone())?,
            ))
        } else {
            callee.call(self.receiver, &self.arguments)
        }
    }
}

struct ScopeFrame<'a> {
    entry: u32,
    plan: &'a ScopeDescriptor,
    environment: Environment,
}

fn binding_frame<'a, 'b>(scopes: &'a [ScopeFrame<'b>], entry: u32) -> &'a ScopeFrame<'b> {
    scopes
        .iter()
        .rev()
        .find(|frame| frame.entry == entry)
        .expect("verified active binding scope")
}

fn enter_scope(
    plan: &ScopeDescriptor,
    parent: Option<&Environment>,
    realm: Option<&Realm>,
    function: Option<&Environment>,
) -> RuntimeResult<Environment> {
    let environment = match plan.kind {
        ScopeKind::Global => realm
            .ok_or_else(|| RuntimeError::type_error("global scope requires a Realm"))?
            .global_environment(),
        ScopeKind::Function => function
            .ok_or_else(|| RuntimeError::type_error("function scope requires a prepared frame"))?
            .clone(),
        ScopeKind::Block => Environment::declarative(parent.cloned()),
    };
    // Check every declaration before mutating the persistent global record.
    if plan.kind == ScopeKind::Global {
        for binding in plan.bindings.iter().filter(|b| b.kind != BindingKind::Var) {
            if !environment.can_declare_global_lexical(&binding.name)? {
                return Err(RuntimeError::syntax_error(
                    "global lexical declaration conflicts",
                ));
            }
        }
        for binding in plan.bindings.iter().filter(|b| b.kind == BindingKind::Var) {
            if environment.has_lexical_declaration(&binding.name)? {
                return Err(RuntimeError::syntax_error(
                    "global var conflicts with lexical binding",
                ));
            }
        }
        for binding in plan.bindings.iter().filter(|b| b.kind == BindingKind::Var) {
            if !environment.can_declare_global_var(&binding.name)? {
                return Err(RuntimeError::type_error(
                    "global object rejects var binding",
                ));
            }
        }
    }
    for binding in plan
        .bindings
        .iter()
        .filter(|b| b.kind != BindingKind::Var)
        .chain(plan.bindings.iter().filter(|b| b.kind == BindingKind::Var))
    {
        if plan.kind == ScopeKind::Global {
            if binding.kind == BindingKind::Var {
                environment.create_global_var_binding(binding.name.clone(), false)?;
            } else {
                environment.create_global_lexical_binding(
                    binding.name.clone(),
                    binding.kind == BindingKind::Let,
                    binding.kind == BindingKind::Const,
                )?;
            }
        } else {
            match binding.kind {
                BindingKind::Var => {
                    if !environment.has_binding(&binding.name)? {
                        environment.create_mutable_binding(binding.name.clone(), false)?;
                        environment.initialize_binding(&binding.name, Value::Undefined)?;
                    }
                }
                BindingKind::Let => {
                    environment.create_mutable_binding(binding.name.clone(), false)?
                }
                BindingKind::Const => {
                    environment.create_immutable_binding(binding.name.clone(), true)?
                }
            }
        }
    }
    Ok(environment)
}

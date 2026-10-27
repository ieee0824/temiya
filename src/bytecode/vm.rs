use crate::runtime::{
    ArrayObject, Completion, Environment, EnvironmentKind, Object, Realm, Reference,
    ReferencedName, RuntimeError, RuntimeResult, Value, binary_values, binary_values_in_realm,
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
                Instruction::CreateObject => stack.push(Value::Object(
                    crate::runtime::object_literal::create(realm.expect("verified object Realm")),
                )),
                Instruction::CoercePropertyKey => {
                    let key = crate::runtime::object_literal::property_key(
                        realm.expect("verified object Realm"),
                        pop(&mut stack)?,
                    )?;
                    stack.push(match key {
                        crate::runtime::PropertyKey::String(name) => Value::String(name),
                        crate::runtime::PropertyKey::Symbol(symbol) => Value::Symbol(symbol),
                    });
                }
                Instruction::DefineObjectProperty { prototype_setter } => {
                    let value = pop(&mut stack)?;
                    let key = pop(&mut stack)?.to_property_key()?;
                    let Value::Object(object) = pop(&mut stack)? else {
                        return Err(RuntimeError::type_error(
                            "object initializer target is not an object",
                        ));
                    };
                    crate::runtime::object_literal::define_data(
                        &object,
                        key,
                        value,
                        prototype_setter,
                    )?;
                    stack.push(Value::Object(object));
                }
                Instruction::SpreadObject => {
                    let source = pop(&mut stack)?;
                    let Value::Object(object) = pop(&mut stack)? else {
                        return Err(RuntimeError::type_error(
                            "object spread target is not an object",
                        ));
                    };
                    crate::runtime::object_literal::spread(&object, source)?;
                    stack.push(Value::Object(object));
                }
                Instruction::CreateArray => {
                    stack.push(Value::Object(crate::runtime::array_literal::create(
                        realm.expect("verified Array Realm"),
                    )?));
                    stack.push(Value::Number(0.0));
                }
                Instruction::AppendArray | Instruction::SpreadArray => {
                    let value = pop(&mut stack)?;
                    let (object, mut next) = array_prefix(&mut stack)?;
                    if matches!(instruction, Instruction::AppendArray) {
                        crate::runtime::array_literal::append(&object, &mut next, value)?;
                    } else {
                        crate::runtime::array_literal::spread(
                            realm.expect("verified Array Realm"),
                            &object,
                            &mut next,
                            value,
                        )?;
                    }
                    stack.push(Value::Object(object));
                    stack.push(Value::Number(next as f64));
                }
                Instruction::ElideArray => {
                    let (object, mut next) = array_prefix(&mut stack)?;
                    crate::runtime::array_literal::elide(&mut next)?;
                    stack.push(Value::Object(object));
                    stack.push(Value::Number(next as f64));
                }
                Instruction::FinishArray => {
                    let (object, next) = array_prefix(&mut stack)?;
                    crate::runtime::array_literal::finish(&object, next)?;
                    stack.push(Value::Object(object));
                }
                Instruction::EnterScope(index) => {
                    let Constant::Scope(plan) = &body.constants[index as usize] else {
                        unreachable!()
                    };
                    let environment = enter_scope(plan, &scopes, (pc - 1) as u32, realm, function)?;
                    let binding_hints =
                        (bytecode.has_backedge() && plan.kind == ScopeKind::Function).then(|| {
                            plan.bindings
                                .iter()
                                .map(|binding| environment.function_binding_index(&binding.name))
                                .collect::<Box<[_]>>()
                        });
                    scopes.push(ScopeFrame {
                        entry: (pc - 1) as u32,
                        plan,
                        environment,
                        binding_hints,
                    });
                }
                Instruction::ExitScope => {
                    scopes.pop();
                }
                Instruction::CloneLoopBindings => {
                    let frame = scopes.last_mut().expect("verified loop environment");
                    let next = Environment::declarative(frame.environment.outer());
                    for binding in &frame.plan.bindings {
                        let value = frame
                            .environment
                            .get_binding_value(&binding.name, frame.plan.strict)?;
                        next.create_mutable_binding(binding.name.clone(), false)?;
                        next.initialize_binding(&binding.name, value)?;
                    }
                    frame.environment = next;
                }
                Instruction::LoadBinding { scope, slot } => {
                    let frame = binding_frame(&scopes, scope);
                    let name = &frame.plan.bindings[slot as usize].name;
                    stack.push(match &frame.binding_hints {
                        Some(hints) => frame.environment.get_function_binding_at(
                            name,
                            hints[slot as usize],
                            frame.plan.strict,
                        ),
                        None => frame.environment.get_binding_value(name, frame.plan.strict),
                    }?);
                }
                Instruction::ResolveBinding {
                    scope,
                    slot,
                    strict,
                } => {
                    let frame = binding_frame(&scopes, scope);
                    references.push(Reference::function_binding(
                        frame.environment.clone(),
                        frame.plan.bindings[slot as usize].name.clone(),
                        strict,
                        frame
                            .binding_hints
                            .as_ref()
                            .and_then(|hints| hints[slot as usize]),
                    ));
                }
                Instruction::UpdateBinding {
                    scope,
                    slot,
                    strict,
                    increment,
                    prefix,
                } => {
                    let frame = binding_frame(&scopes, scope);
                    let reference = Reference::function_binding(
                        frame.environment.clone(),
                        frame.plan.bindings[slot as usize].name.clone(),
                        strict,
                        frame
                            .binding_hints
                            .as_ref()
                            .and_then(|hints| hints[slot as usize]),
                    );
                    stack.push(crate::runtime::update_reference_in_realm(
                        realm.expect("scoped update Realm"),
                        &reference,
                        increment,
                        prefix,
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
                    let name = &frame.plan.bindings[slot as usize].name;
                    let value = pop(&mut stack)?;
                    match &frame.binding_hints {
                        Some(hints) => frame.environment.set_function_binding_at(
                            name,
                            hints[slot as usize],
                            value,
                            frame.plan.strict,
                        ),
                        None => {
                            frame
                                .environment
                                .set_mutable_binding(name, value, frame.plan.strict)
                        }
                    }?;
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
                Instruction::GetName {
                    name: index,
                    strict,
                } => {
                    let Constant::String(name) = &body.constants[index as usize] else {
                        unreachable!()
                    };
                    stack.push(
                        scopes
                            .last()
                            .expect("verified active scope")
                            .environment
                            .get_identifier_value(name, strict)?,
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
                Instruction::LoadReference => {
                    stack.push(
                        references
                            .last()
                            .expect("verified reference stack")
                            .get_value()?,
                    );
                }
                Instruction::DiscardReference => {
                    references.pop().expect("verified reference stack");
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
                Instruction::UpdateReference { increment, prefix } => {
                    let reference = references.pop().expect("verified reference stack");
                    stack.push(crate::runtime::update_reference_in_realm(
                        realm.expect("scoped update Realm"),
                        &reference,
                        increment,
                        prefix,
                    )?);
                }
                Instruction::TypeOfReference => {
                    let reference = references.pop().expect("verified reference stack");
                    let value = if reference.is_unresolvable() {
                        Value::Undefined
                    } else {
                        reference.get_value()?
                    };
                    stack.push(Value::String(crate::runtime::type_of(&value).into()));
                }
                Instruction::UnaryNumeric(operator) => {
                    let value = pop(&mut stack)?;
                    stack.push(crate::runtime::unary_numeric_value(
                        realm,
                        operator.spelling(),
                        value,
                    )?);
                }
                Instruction::TypeOf => {
                    let value = pop(&mut stack)?;
                    stack.push(Value::String(crate::runtime::type_of(&value).into()));
                }
                Instruction::DeleteReference => {
                    stack.push(Value::Boolean(
                        references
                            .pop()
                            .expect("verified reference stack")
                            .delete()?,
                    ));
                }
                Instruction::LoadReferenceCallTarget => {
                    let reference = references.pop().expect("verified reference stack");
                    let receiver = reference.this_value().unwrap_or(Value::Undefined);
                    let callee = reference.get_value()?;
                    stack.push(receiver);
                    stack.push(callee);
                }
                Instruction::AsCallTarget => {
                    let callee = pop(&mut stack)?;
                    stack.push(Value::Undefined);
                    stack.push(callee);
                }
                Instruction::BeginMethodCall => {
                    let callee = pop(&mut stack)?;
                    let receiver = pop(&mut stack)?;
                    calls.push(Invocation {
                        callee,
                        receiver,
                        construct: false,
                        arguments: vec![],
                    });
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
                    crate::runtime::iterator::push_argument(
                        &mut calls.last_mut().expect("verified call frame").arguments,
                        pop(&mut stack)?,
                    )?;
                }
                Instruction::SpreadArguments => {
                    crate::runtime::iterator::spread_arguments(
                        realm.expect("verified scoped spread"),
                        pop(&mut stack)?,
                        &mut calls.last_mut().expect("verified call frame").arguments,
                    )?;
                }
                Instruction::Invoke => {
                    let invocation = calls.pop().expect("verified call frame");
                    stack.push(invocation.invoke()?);
                }
                Instruction::CreateFunction(index) => {
                    let Constant::Function(template) = &body.constants[index as usize] else {
                        unreachable!()
                    };
                    stack.push(Value::Object(crate::runtime::bytecode_function::create(
                        template.clone(),
                        realm.expect("verified function Realm"),
                        scopes
                            .last()
                            .expect("verified active scope")
                            .environment
                            .clone(),
                    )?));
                }
                Instruction::LoadNewTarget => {
                    let mut environment = scopes
                        .last()
                        .expect("verified active scope")
                        .environment
                        .clone();
                    loop {
                        if environment.kind() == EnvironmentKind::Global {
                            stack.push(Value::Undefined);
                            break;
                        }
                        if environment.kind() == EnvironmentKind::Function
                            && environment.has_this_binding()?
                        {
                            stack.push(
                                environment
                                    .new_target()?
                                    .map_or(Value::Undefined, Value::Object),
                            );
                            break;
                        }
                        environment = environment.outer().ok_or_else(|| {
                            RuntimeError::reference_error("no newTarget environment")
                        })?;
                    }
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
                Instruction::Binary(operator) => binary(&mut stack, operator.spelling(), realm)?,
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
        Constant::Scope(_) | Constant::Function(_) => {
            unreachable!("verifier rejects metadata values")
        }
        Constant::BigInt(value) => Value::BigInt(value.clone()),
        Constant::Undefined => Value::Undefined,
        Constant::Null => Value::Null,
        Constant::Boolean(value) => Value::Boolean(*value),
        Constant::Number(value) => Value::Number(*value),
        Constant::String(value) => Value::String(value.clone()),
    }
}

fn array_prefix(stack: &mut Vec<Value>) -> RuntimeResult<(Object, u64)> {
    let Value::Number(next) = pop(stack)? else {
        return Err(RuntimeError::type_error(
            "Array initializer index is not a number",
        ));
    };
    let Value::Object(object) = pop(stack)? else {
        return Err(RuntimeError::type_error(
            "Array initializer target is not an object",
        ));
    };
    if ArrayObject::from_object(object.clone()).is_none()
        || !next.is_finite()
        || next < 0.0
        || next.fract() != 0.0
        || next > 9_007_199_254_740_991.0
    {
        return Err(RuntimeError::type_error("invalid Array initializer state"));
    }
    Ok((object, next as u64))
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
    binding_hints: Option<Box<[Option<usize>]>>,
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
    parents: &[ScopeFrame<'_>],
    entry: u32,
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
        ScopeKind::Block => {
            Environment::declarative(parents.last().map(|frame| frame.environment.clone()))
        }
    };
    // Global eligibility is atomic with respect to binding/function creation.
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
        for declaration in plan.functions.iter().rev() {
            if !environment.can_declare_global_function(&declaration.name)? {
                return Err(RuntimeError::type_error(
                    "global object rejects function binding",
                ));
            }
        }
        for binding in plan.bindings.iter().filter(|b| {
            b.kind == BindingKind::Var && !plan.functions.iter().any(|f| f.name == b.name)
        }) {
            if !environment.can_declare_global_var(&binding.name)? {
                return Err(RuntimeError::type_error(
                    "global object rejects var binding",
                ));
            }
        }
        for binding in plan.bindings.iter().filter(|b| b.kind != BindingKind::Var) {
            environment.create_global_lexical_binding(
                binding.name.clone(),
                binding.kind == BindingKind::Let,
                binding.kind == BindingKind::Const,
            )?;
        }
        instantiate_functions(plan, parents, entry, &environment, realm)?;
        for binding in plan.bindings.iter().filter(|b| {
            b.kind == BindingKind::Var && !plan.functions.iter().any(|f| f.name == b.name)
        }) {
            environment.create_global_var_binding(binding.name.clone(), false)?;
        }
        return Ok(environment);
    }
    for binding in plan
        .bindings
        .iter()
        .filter(|b| b.kind != BindingKind::Var)
        .chain(plan.bindings.iter().filter(|b| b.kind == BindingKind::Var))
    {
        match binding.kind {
            BindingKind::Var => {
                if !environment.has_binding(&binding.name)? {
                    environment.create_mutable_binding(binding.name.clone(), false)?;
                    environment.initialize_binding(&binding.name, Value::Undefined)?;
                }
            }
            BindingKind::Let => environment.create_mutable_binding(binding.name.clone(), false)?,
            BindingKind::Const => {
                environment.create_immutable_binding(binding.name.clone(), true)?
            }
        }
    }
    instantiate_functions(plan, parents, entry, &environment, realm)?;
    Ok(environment)
}

fn instantiate_functions(
    plan: &ScopeDescriptor,
    parents: &[ScopeFrame<'_>],
    entry: u32,
    environment: &Environment,
    realm: Option<&Realm>,
) -> RuntimeResult<()> {
    for declaration in &plan.functions {
        let function = crate::runtime::bytecode_function::create(
            declaration.template.clone(),
            realm.expect("scoped declaration Realm"),
            environment.clone(),
        )?;
        let target = if declaration.target == entry {
            environment
        } else {
            &binding_frame(parents, declaration.target).environment
        };
        if target.kind() == EnvironmentKind::Global {
            target.create_global_function_binding(
                declaration.name.clone(),
                Value::Object(function),
                false,
            )?;
        } else if declaration.target == entry && plan.kind == ScopeKind::Block {
            target.initialize_binding(&declaration.name, Value::Object(function))?;
        } else {
            target.set_mutable_binding(&declaration.name, Value::Object(function), false)?;
        }
    }
    Ok(())
}

use crate::runtime::{
    ArrayObject, Completion, Environment, EnvironmentKind, Object, PropertyReferenceContext, Realm,
    Reference, ReferencedName, RuntimeError, RuntimeResult, Value, binary_number, binary_values,
    binary_values_in_realm,
};

use super::{
    BinaryOperator, BindingKind, Constant, Instruction, ScopeDescriptor, ScopeKind,
    VerifiedBytecode,
};

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
        Self::run::<false>(bytecode, None, None, false)
    }

    /// Executes Script bytecode in a persistent host-supplied Realm.
    pub fn execute_in_realm(
        bytecode: &VerifiedBytecode,
        realm: &Realm,
    ) -> RuntimeResult<Completion> {
        Self::run::<false>(bytecode, Some(realm), None, false)
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
        Self::run::<false>(bytecode, Some(realm), Some(frame), false)
    }

    // This entry never treats the captured outer environment as a private
    // activation: scalar-local clearing and loop-cell reuse remain disabled.
    pub(crate) fn execute_without_own_frame(
        bytecode: &VerifiedBytecode,
        realm: &Realm,
        outer: &Environment,
        this_value: &Value,
    ) -> RuntimeResult<Completion> {
        if !bytecode.can_elide_empty_function_frame() {
            return Err(RuntimeError::internal_error(
                "function frame elision was not verified",
            ));
        }
        if bytecode.has_short_function_path() {
            return super::short_function::execute(bytecode, realm, outer, this_value);
        }
        Self::run::<false>(bytecode, Some(realm), Some(outer), false)
    }

    // Internal calls own their freshly instantiated frame. Scalar binding
    // elision additionally requires unobservable bindings; safe loop-cell reuse
    // is proven independently. Public prepared frames retain their old path.
    pub(crate) fn execute_with_private_frame(
        bytecode: &VerifiedBytecode,
        realm: &Realm,
        frame: &Environment,
        scalar_bindings_unobserved: bool,
    ) -> RuntimeResult<Completion> {
        if scalar_bindings_unobserved && scalar_loop_locals(bytecode) {
            Self::run::<true>(bytecode, Some(realm), Some(frame), true)
        } else {
            Self::run::<false>(
                bytecode,
                Some(realm),
                Some(frame),
                bytecode.reuse_loop_bindings,
            )
        }
    }

    fn run<const SCALAR_LOCALS: bool>(
        bytecode: &VerifiedBytecode,
        realm: Option<&Realm>,
        function: Option<&Environment>,
        reuse_loop_bindings: bool,
    ) -> RuntimeResult<Completion> {
        if bytecode.has_catch_handlers {
            Self::run_catches::<SCALAR_LOCALS, true>(bytecode, realm, function, reuse_loop_bindings)
        } else {
            Self::run_catches::<SCALAR_LOCALS, false>(
                bytecode,
                realm,
                function,
                reuse_loop_bindings,
            )
        }
    }

    fn run_catches<const SCALAR_LOCALS: bool, const HANDLERS: bool>(
        bytecode: &VerifiedBytecode,
        realm: Option<&Realm>,
        function: Option<&Environment>,
        reuse_loop_bindings: bool,
    ) -> RuntimeResult<Completion> {
        let _object_roots = realm.and_then(Realm::enter_object_root_region);
        let mut property_context = realm.map(PropertyReferenceContext::new);
        let mut definition_context = bytecode.definition_context();
        if bytecode.definition_function_root {
            if let Some(function) = function {
                definition_context.pin_function_frame(function)?;
            }
        }
        let body = bytecode.bytecode();
        let mut scalar_scopes: Vec<Option<ScalarScope<'_>>> = Vec::new();
        if SCALAR_LOCALS {
            scalar_scopes.resize_with(body.instructions.len(), || None);
        }
        let mut scopes: SingleStack<ScopeFrame<'_>> = SingleStack::new();
        let mut references: SingleStack<Reference> = SingleStack::new();
        let mut calls: SingleStack<Invocation> = SingleStack::new();
        let mut stack = Vec::with_capacity(bytecode.max_stack());
        let mut locals = vec![Value::Undefined; body.local_count as usize];
        let mut property_hints = bytecode
            .has_own_property_cache()
            .then(|| vec![None; body.instructions.len()]);
        let mut pc = 0;
        let mut handlers: Vec<(usize, usize, usize)> = Vec::new();
        loop {
            let result = (|| -> RuntimeResult<Completion> {
                loop {
                    while HANDLERS
                        && handlers
                            .last()
                            .is_some_and(|(start, end, _)| pc < *start || pc >= *end)
                    {
                        handlers.pop();
                    }
                    let instruction = body.instructions[pc];
                    pc += 1;
                    match instruction {
                        Instruction::BeginTryCatch(target) => {
                            assert!(HANDLERS, "verified handler flag");
                            handlers.push((pc, target as usize, scopes.len()))
                        }
                        Instruction::CreateObject => {
                            stack.push(Value::Object(crate::runtime::object_literal::create(
                                realm.expect("verified object Realm"),
                            )))
                        }
                        Instruction::CreateObjectReserved(count) => stack.push(Value::Object(
                            crate::runtime::object_literal::create_reserved(
                                realm.expect("verified object Realm"),
                                count,
                            ),
                        )),
                        Instruction::CoercePropertyKey => {
                            let key = crate::runtime::object_literal::property_key(
                                realm.expect("verified object Realm"),
                                pop(&mut stack)?,
                            )?;
                            stack.push(match key {
                                crate::runtime::PropertyKey::String(name) => Value::String(name),
                                crate::runtime::PropertyKey::Symbol(symbol) => {
                                    Value::Symbol(symbol)
                                }
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
                        Instruction::DefineNamedObjectProperty {
                            name,
                            prototype_setter,
                        } => {
                            let value = pop(&mut stack)?;
                            let Value::Object(object) = pop(&mut stack)? else {
                                return Err(RuntimeError::type_error(
                                    "object initializer target is not an object",
                                ));
                            };
                            let Constant::String(name) = &body.constants[name as usize] else {
                                unreachable!("verified String property name")
                            };
                            crate::runtime::object_literal::define_data(
                                &object,
                                name.clone().into(),
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
                            // The whole-body proof excludes all operations that can observe
                            // block environments or retain bindings. Keep primitive slots and
                            // TDZ state locally; loop-environment cloning has no observer.
                            let fused_initialization = !SCALAR_LOCALS
                                && plan.kind == ScopeKind::Block
                                && plan.bindings.len() == 1
                                && plan.bindings[0].kind != BindingKind::Var
                                && plan.functions.is_empty()
                                && matches!(body.instructions.get(pc),
                                    Some(Instruction::InitializeBinding {scope, slot: 0})
                                        if *scope == (pc - 1) as u32);
                            let environment = if fused_initialization {
                                Environment::declarative_with_verified_initial_value(
                                    scopes
                                        .last()
                                        .expect("verified block parent")
                                        .environment
                                        .clone(),
                                    plan.bindings[0].name.clone(),
                                    plan.bindings[0].kind != BindingKind::Const,
                                    pop(&mut stack)?,
                                )?
                            } else if SCALAR_LOCALS && plan.kind == ScopeKind::Block {
                                function.expect("private function frame").clone()
                            } else {
                                enter_scope(plan, &scopes, (pc - 1) as u32, realm, function)?
                            };
                            if SCALAR_LOCALS {
                                let values = plan
                                    .bindings
                                    .iter()
                                    .map(|binding| {
                                        if binding.kind == BindingKind::Var {
                                            environment
                                                .get_binding_value(&binding.name, plan.strict)
                                                .map(Some)
                                        } else {
                                            Ok(None)
                                        }
                                    })
                                    .collect::<RuntimeResult<Box<[_]>>>()?;
                                scalar_scopes[pc - 1] = Some(ScalarScope { plan, values });
                            }
                            let binding_hints = (!SCALAR_LOCALS
                                && bytecode.has_backedge()
                                && plan.kind == ScopeKind::Function)
                                .then(|| {
                                    plan.bindings
                                        .iter()
                                        .map(|binding| {
                                            environment.function_binding_index(&binding.name)
                                        })
                                        .collect::<Box<[_]>>()
                                });
                            scopes.push(ScopeFrame {
                                entry: (pc - 1) as u32,
                                plan,
                                environment,
                                binding_hints,
                            });
                            if fused_initialization {
                                pc += 1;
                            }
                        }
                        Instruction::ExitScope => {
                            scopes.pop();
                        }
                        Instruction::CloneLoopBindings => {
                            if reuse_loop_bindings {
                                continue;
                            }
                            let frame = scopes.last_mut().expect("verified loop environment");
                            let next = frame.environment.clone_verified_loop_bindings(
                                frame.plan.bindings.iter().map(|binding| &binding.name),
                            )?;
                            frame.environment = next;
                        }
                        Instruction::LoadBinding { scope, slot } => {
                            if SCALAR_LOCALS {
                                let slots = scalar_scopes[scope as usize]
                                    .as_ref()
                                    .expect("active scalar scope");
                                stack.push(match slots.read(slot as usize)? {
                                    Value::Number(value) => Value::Number(*value),
                                    value => value.clone(),
                                });
                                continue;
                            }
                            let frame = binding_frame(&scopes, scope);
                            let name = &frame.plan.bindings[slot as usize].name;
                            stack.push(match &frame.binding_hints {
                                Some(hints) => frame.environment.get_function_binding_at(
                                    name,
                                    hints[slot as usize],
                                    frame.plan.strict,
                                ),
                                None => {
                                    frame.environment.get_binding_value(name, frame.plan.strict)
                                }
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
                            if SCALAR_LOCALS {
                                let slots = scalar_scopes[scope as usize]
                                    .as_mut()
                                    .expect("active scalar scope");
                                let old = match slots.read(slot as usize)? {
                                    Value::Number(value) => *value,
                                    value => value.to_number()?,
                                };
                                let new = if increment { old + 1.0 } else { old - 1.0 };
                                slots.write(slot as usize, Value::Number(new))?;
                                stack.push(Value::Number(if prefix { new } else { old }));
                                continue;
                            }
                            let frame = binding_frame(&scopes, scope);
                            let name = &frame.plan.bindings[slot as usize].name;
                            let hint = frame
                                .binding_hints
                                .as_ref()
                                .and_then(|hints| hints[slot as usize]);
                            if let Some((old, new)) = frame
                                .environment
                                .update_number_binding(name, hint, increment, strict)?
                            {
                                stack.push(Value::Number(if prefix { new } else { old }));
                                continue;
                            }
                            let reference = Reference::function_binding(
                                frame.environment.clone(),
                                name.clone(),
                                strict,
                                hint,
                            );
                            stack.push(crate::runtime::update_reference_in_realm(
                                realm.expect("scoped update Realm"),
                                &reference,
                                increment,
                                prefix,
                            )?);
                        }
                        Instruction::AppendStringBinding { scope, slot, value } => {
                            let frame = binding_frame(&scopes, scope);
                            let name = &frame.plan.bindings[slot as usize].name;
                            let hint = frame
                                .binding_hints
                                .as_ref()
                                .and_then(|hints| hints[slot as usize]);
                            let Constant::String(suffix) = &body.constants[value as usize] else {
                                unreachable!()
                            };
                            if let Some(value) =
                                frame.environment.append_string_binding(name, hint, suffix)
                            {
                                stack.push(value);
                            } else {
                                let reference = Reference::function_binding(
                                    frame.environment.clone(),
                                    name.clone(),
                                    frame.plan.strict,
                                    hint,
                                );
                                let value = binary_values_in_realm(
                                    realm.expect("scoped string append Realm"),
                                    "+",
                                    reference.get_value()?,
                                    Value::String(suffix.clone()),
                                )?;
                                reference.put_value(value.clone())?;
                                stack.push(value);
                            }
                        }
                        Instruction::InitializeBinding { scope, slot } => {
                            if SCALAR_LOCALS {
                                let slots = scalar_scopes[scope as usize]
                                    .as_mut()
                                    .expect("active scalar scope");
                                let value = pop(&mut stack)?;
                                if slots.values[slot as usize].is_some() {
                                    return Err(RuntimeError::type_error(
                                        "binding is already initialized",
                                    ));
                                }
                                slots.values[slot as usize] = Some(value);
                                continue;
                            }
                            let frame = binding_frame(&scopes, scope);
                            frame.environment.initialize_binding(
                                &frame.plan.bindings[slot as usize].name,
                                pop(&mut stack)?,
                            )?;
                        }
                        Instruction::StoreBinding { scope, slot } => {
                            let value = pop(&mut stack)?;
                            if SCALAR_LOCALS {
                                scalar_scopes[scope as usize]
                                    .as_mut()
                                    .expect("active scalar scope")
                                    .write(slot as usize, value)?;
                                continue;
                            }
                            let frame = binding_frame(&scopes, scope);
                            let name = &frame.plan.bindings[slot as usize].name;
                            match &frame.binding_hints {
                                Some(hints) => frame.environment.set_function_binding_at(
                                    name,
                                    hints[slot as usize],
                                    value,
                                    frame.plan.strict,
                                ),
                                None => frame.environment.set_mutable_binding(
                                    name,
                                    value,
                                    frame.plan.strict,
                                ),
                            }?;
                        }
                        Instruction::LoadName(index) | Instruction::GetName { name: index, .. } => {
                            let Constant::String(name) = &body.constants[index as usize] else {
                                unreachable!()
                            };
                            let strict = match instruction {
                                Instruction::GetName { strict, .. } => strict,
                                _ => true,
                            };
                            let active = &scopes.last().expect("verified active scope").environment;
                            let environment = if bytecode.function_name_lookup[pc - 1] {
                                function
                                    .filter(|environment| {
                                        environment.kind() == EnvironmentKind::Function
                                    })
                                    .unwrap_or(active)
                            } else {
                                active
                            };
                            stack.push(environment.get_identifier_value(name, strict)?);
                        }
                        Instruction::ResolveName {
                            name: index,
                            strict,
                        } => {
                            let Constant::String(name) = &body.constants[index as usize] else {
                                unreachable!()
                            };
                            let environment =
                                &scopes.last().expect("verified active scope").environment;
                            let reference = if let Some((binding, hint)) =
                                environment.resolve_binding_with_hint(name)?
                            {
                                Reference::function_binding(binding, name.clone(), strict, hint)
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
                        Instruction::GetNamedProperty { name, strict }
                        | Instruction::ResolveNamedProperty { name, strict } => {
                            let Constant::String(name) = &body.constants[name as usize] else {
                                unreachable!()
                            };
                            let base = pop(&mut stack)?;
                            if matches!(instruction, Instruction::GetNamedProperty { .. }) {
                                if let Some(value) = primitive_named_value(&base, name) {
                                    stack.push(value);
                                    continue;
                                }
                            }
                            let key = crate::runtime::PropertyKey::String(name.clone());
                            if matches!(instruction, Instruction::GetNamedProperty { .. }) {
                                if let (Some(hints), Value::Object(object)) =
                                    (&mut property_hints, &base)
                                {
                                    let hint = &mut hints[pc - 1];
                                    if let Some((value, index)) =
                                        object.own_data_value_with_hint(&key, *hint)
                                    {
                                        *hint = Some(index);
                                        stack.push(value);
                                        continue;
                                    }
                                    *hint = None;
                                }
                            }
                            let reference = property_context
                                .as_mut()
                                .expect("verified scoped property access")
                                .reference(base, ReferencedName::PropertyKey(key), strict);
                            if matches!(instruction, Instruction::GetNamedProperty { .. }) {
                                stack.push(reference.get_value()?);
                            } else {
                                references.push(reference);
                            }
                        }
                        Instruction::GetProperty { strict }
                        | Instruction::ResolveProperty { strict } => {
                            let key = pop(&mut stack)?;
                            let base = pop(&mut stack)?;
                            if matches!(instruction, Instruction::GetProperty { .. }) {
                                if let Some(value) = indexed_value(&base, &key) {
                                    stack.push(value);
                                    continue;
                                }
                            }
                            let reference = property_context
                                .as_mut()
                                .expect("verified scoped property access")
                                .reference(base, ReferencedName::Value(key), strict);
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
                                arguments: CallArguments::new(),
                            });
                        }
                        Instruction::BeginCall | Instruction::BeginConstruct => {
                            calls.push(Invocation {
                                callee: pop(&mut stack)?,
                                receiver: Value::Undefined,
                                construct: matches!(instruction, Instruction::BeginConstruct),
                                arguments: CallArguments::new(),
                            });
                        }
                        Instruction::BeginReferenceCall => {
                            let reference = references.pop().expect("verified reference stack");
                            let receiver = reference.this_value().unwrap_or(Value::Undefined);
                            calls.push(Invocation {
                                callee: reference.get_value()?,
                                receiver,
                                construct: false,
                                arguments: CallArguments::new(),
                            });
                        }
                        Instruction::AppendArgument => {
                            calls
                                .last_mut()
                                .expect("verified call frame")
                                .arguments
                                .push(pop(&mut stack)?)?;
                        }
                        Instruction::SpreadArguments => {
                            crate::runtime::iterator::spread_arguments(
                                realm.expect("verified scoped spread"),
                                pop(&mut stack)?,
                                calls
                                    .last_mut()
                                    .expect("verified call frame")
                                    .arguments
                                    .as_vec_mut(),
                            )?;
                        }
                        Instruction::Invoke => {
                            let invocation = calls.pop().expect("verified call frame");
                            stack.push(invocation.invoke()?);
                        }
                        Instruction::ExecuteTry(index) => {
                            let Constant::TryStatement { definition, strict } =
                                &body.constants[index as usize]
                            else {
                                unreachable!("verified try")
                            };
                            let completion = definition_context.evaluate_try(
                                definition,
                                *strict,
                                realm.expect("try Realm"),
                                &scopes.last().expect("try scope").environment,
                                function.expect("try function"),
                            )?;
                            if !matches!(completion, Completion::Normal(_)) {
                                return Ok(completion);
                            }
                        }
                        Instruction::InitializePattern(index) => {
                            let Constant::BindingPattern {
                                definition,
                                initialize,
                                strict,
                            } = &body.constants[index as usize]
                            else {
                                unreachable!("verified binding pattern")
                            };
                            let value = pop(&mut stack)?;
                            if let (Some(plan), Value::Object(source)) =
                                (&bytecode.static_object_patterns[index as usize], &value)
                            {
                                plan.initialize(
                                    &scopes.last().expect("verified pattern scope").environment,
                                    source,
                                )?;
                                continue;
                            }
                            definition_context.initialize_pattern(
                                definition,
                                value,
                                *initialize,
                                *strict,
                                realm.expect("verified pattern Realm"),
                                &scopes.last().expect("verified pattern scope").environment,
                            )?;
                        }
                        Instruction::CreateObjectLiteral(index) => {
                            let Constant::ObjectLiteral { definition, strict } =
                                &body.constants[index as usize]
                            else {
                                unreachable!()
                            };
                            stack.push(definition_context.evaluate_object_literal(
                                definition,
                                *strict,
                                realm.expect("verified object initializer Realm"),
                                &scopes.last().expect("verified active scope").environment,
                            )?);
                        }
                        Instruction::CreateClass(index) => {
                            let Constant::Class {
                                definition,
                                inferred_name,
                            } = &body.constants[index as usize]
                            else {
                                unreachable!()
                            };
                            stack.push(Value::Object(definition_context.evaluate(
                                definition,
                                inferred_name.clone(),
                                realm.expect("verified class Realm"),
                                &scopes.last().expect("verified active scope").environment,
                            )?));
                        }
                        Instruction::CreateFunction(index) => {
                            let Constant::Function(template) = &body.constants[index as usize]
                            else {
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
                        Instruction::LoadSuperBinding => {
                            let (receiver, function) = scopes
                                .last()
                                .expect("verified super scope")
                                .environment
                                .nearest_super_binding()?;
                            stack.push(receiver);
                            stack.push(Value::Object(function));
                        }
                        Instruction::ResolveSuperProperty { strict } => {
                            let name = ReferencedName::Value(pop(&mut stack)?);
                            let Value::Object(function) = pop(&mut stack)? else {
                                return Err(RuntimeError::type_error(
                                    "super reference requires a function",
                                ));
                            };
                            let receiver = pop(&mut stack)?;
                            references.push(Reference::super_property_in_realm(
                                realm.expect("verified super Realm"),
                                function.super_base()?,
                                name,
                                strict,
                                receiver,
                            ));
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
                                if environment.kind() == EnvironmentKind::Function
                                    && environment.has_this_binding()?
                                {
                                    stack.push(environment.get_this_binding()?);
                                    break;
                                }
                                environment = environment.outer().ok_or_else(|| {
                                    RuntimeError::reference_error("no this binding")
                                })?;
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
                        Instruction::Duplicate => {
                            stack.push(stack.last().cloned().ok_or_else(|| {
                                RuntimeError::internal_error("verified stack was empty")
                            })?)
                        }
                        Instruction::Pop => {
                            pop(&mut stack)?;
                        }
                        Instruction::Binary(operator) => binary(&mut stack, operator, realm)?,
                        Instruction::Add => binary(&mut stack, BinaryOperator::Add, realm)?,
                        Instruction::Subtract => {
                            binary(&mut stack, BinaryOperator::Subtract, realm)?
                        }
                        Instruction::Multiply => {
                            binary(&mut stack, BinaryOperator::Multiply, realm)?
                        }
                        Instruction::Divide => binary(&mut stack, BinaryOperator::Divide, realm)?,
                        Instruction::StrictEqual => {
                            binary(&mut stack, BinaryOperator::StrictEqual, realm)?
                        }
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
                        Instruction::Complete => {
                            return Ok(Completion::Normal(Some(pop(&mut stack)?)));
                        }
                        Instruction::Throw => return Ok(Completion::Throw(pop(&mut stack)?)),
                    }
                }
            })();
            if !HANDLERS {
                return result;
            }
            let Some((_, target, scope_depth)) = handlers.pop() else {
                return result;
            };
            let thrown = match result {
                Ok(Completion::Throw(value)) => value,
                Err(error) => realm.expect("handler Realm").exception_value(&error)?,
                other => return other,
            };
            stack.clear();
            while scopes.len() > scope_depth {
                scopes.pop();
            }
            while references.pop().is_some() {}
            while calls.pop().is_some() {}
            stack.push(thrown);
            pc = target;
        }
    }
}

fn primitive_named_value(base: &Value, name: &crate::runtime::JsString) -> Option<Value> {
    match base {
        Value::String(string)
            if name
                .code_units()
                .iter()
                .copied()
                .eq("length".encode_utf16()) =>
        {
            Some(Value::Number(string.len() as f64))
        }
        _ => None,
    }
}

fn indexed_value(base: &Value, key: &Value) -> Option<Value> {
    let Value::Number(index) = key else {
        return None;
    };
    if !(*index >= 0.0 && *index < f64::from(u32::MAX)) {
        return None;
    }
    let integer = *index as u32;
    if f64::from(integer) != *index {
        return None;
    }
    match base {
        Value::Object(object) => object.own_element_value(integer),
        Value::String(string) => string
            .code_units()
            .get(integer as usize)
            .map(|unit| Value::String(crate::runtime::JsString::from_code_units_slice(&[*unit]))),
        _ => None,
    }
}

pub(super) fn constant(constant: &Constant) -> Value {
    match constant {
        Constant::Scope(_)
        | Constant::Function(_)
        | Constant::Class { .. }
        | Constant::ObjectLiteral { .. }
        | Constant::BindingPattern { .. }
        | Constant::TryStatement { .. } => {
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

#[inline]
fn binary(
    stack: &mut Vec<Value>,
    operator: BinaryOperator,
    realm: Option<&Realm>,
) -> RuntimeResult<()> {
    // Primitive Number operations cannot call user code. Reuse their operand
    // slots, preserving any roots below them and avoiding a pop/pop/push cycle.
    if let Some([Value::Number(left), Value::Number(right)]) = stack.last_chunk::<2>() {
        if let Some(value) = binary_number(operator, *left, *right) {
            let result_slot = stack.len() - 2;
            stack.truncate(result_slot + 1);
            stack[result_slot] = value;
            return Ok(());
        }
    }
    // Strict equality never coerces or invokes user code, even for proxies.
    // Keep the shared semantics, but avoid string-based/Realm dispatch and
    // pop/pop/push for nonnumeric operands too.
    if matches!(
        operator,
        BinaryOperator::StrictEqual | BinaryOperator::StrictNotEqual
    ) {
        let left = stack.len() - 2;
        let equal = crate::runtime::strict_equal(&stack[left], &stack[left + 1]);
        stack.truncate(left + 1);
        stack[left] = Value::Boolean(equal == (operator == BinaryOperator::StrictEqual));
        return Ok(());
    }
    let right = pop(stack)?;
    let left = pop(stack)?;
    stack.push(if let Some(realm) = realm {
        binary_values_in_realm(realm, operator.spelling(), left, right)?
    } else {
        binary_values(operator.spelling(), left, right)?
    });
    Ok(())
}

// The common single-frame case owns its value directly. Nested scopes,
// references and pending calls spill without cloning or hiding their roots.
enum SingleStack<T> {
    Empty,
    One(T),
    Heap(Vec<T>),
}
impl<T> SingleStack<T> {
    fn new() -> Self {
        Self::Empty
    }
    fn push(&mut self, value: T) {
        match self {
            Self::Empty => *self = Self::One(value),
            Self::Heap(values) => values.push(value),
            Self::One(_) => {
                let Self::One(previous) = std::mem::replace(self, Self::Empty) else {
                    unreachable!()
                };
                *self = Self::Heap(vec![previous, value]);
            }
        }
    }
    fn pop(&mut self) -> Option<T> {
        match self {
            Self::Empty => None,
            Self::Heap(values) => values.pop(),
            Self::One(_) => {
                let Self::One(value) = std::mem::replace(self, Self::Empty) else {
                    unreachable!()
                };
                Some(value)
            }
        }
    }
}
impl<T> std::ops::Deref for SingleStack<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        match self {
            Self::Empty => &[],
            Self::One(value) => std::slice::from_ref(value),
            Self::Heap(values) => values,
        }
    }
}
impl<T> std::ops::DerefMut for SingleStack<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        match self {
            Self::Empty => &mut [],
            Self::One(value) => std::slice::from_mut(value),
            Self::Heap(values) => values,
        }
    }
}

#[cfg(test)]
mod auxiliary_storage_tests {
    use super::*;

    #[test]
    fn single_and_nested_auxiliary_stacks_own_and_release_their_roots() {
        for count in [1, 3] {
            let runtime = crate::runtime::Runtime::new();
            runtime.set_allocation_collection_interval(usize::MAX);
            let mut stack = SingleStack::new();
            for _ in 0..count {
                stack.push(Object::new_in(&runtime));
            }
            assert_eq!(runtime.collect_garbage().unwrap().retained, count);
            drop(stack.pop());
            assert_eq!(runtime.collect_garbage().unwrap().retained, count - 1);
            drop(stack);
            assert_eq!(runtime.collect_garbage().unwrap().retained, 0);
        }
    }
}

// Each invocation owns its roots independently of operand-stack growth and of
// nested/reentrant VM runs. All pending roots drop on an abrupt runtime result.
enum CallArguments {
    Inline { values: [Value; 2], len: usize },
    Heap(Vec<Value>),
}

impl CallArguments {
    fn new() -> Self {
        Self::Inline {
            values: [Value::Undefined, Value::Undefined],
            len: 0,
        }
    }

    fn push(&mut self, value: Value) -> RuntimeResult<()> {
        if let Self::Inline { values, len } = self {
            if *len < values.len() {
                values[*len] = value;
                *len += 1;
                return Ok(());
            }
        }
        crate::runtime::iterator::push_argument(self.as_vec_mut(), value)
    }

    fn as_vec_mut(&mut self) -> &mut Vec<Value> {
        if matches!(self, Self::Inline { .. }) {
            let Self::Inline { values, len } = std::mem::replace(self, Self::Heap(Vec::new()))
            else {
                unreachable!()
            };
            let mut arguments = Vec::with_capacity(4);
            arguments.extend(values.into_iter().take(len));
            *self = Self::Heap(arguments);
        }
        let Self::Heap(arguments) = self else {
            unreachable!()
        };
        arguments
    }

    fn as_slice(&self) -> &[Value] {
        match self {
            Self::Inline { values, len } => &values[..*len],
            Self::Heap(arguments) => arguments,
        }
    }
}

struct Invocation {
    callee: Value,
    receiver: Value,
    construct: bool,
    arguments: CallArguments,
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
                callee.construct(self.arguments.as_slice(), callee.clone())?,
            ))
        } else {
            callee.call(self.receiver, self.arguments.as_slice())
        }
    }
}

struct ScalarScope<'a> {
    plan: &'a ScopeDescriptor,
    values: Box<[Option<Value>]>,
}

impl ScalarScope<'_> {
    fn read(&self, slot: usize) -> RuntimeResult<&Value> {
        self.values[slot].as_ref().ok_or_else(|| {
            RuntimeError::reference_error(format!(
                "{} is in the temporal dead zone",
                self.plan.bindings[slot].name.to_string_lossy()
            ))
        })
    }

    fn write(&mut self, slot: usize, value: Value) -> RuntimeResult<()> {
        self.read(slot)?;
        if self.plan.bindings[slot].kind == BindingKind::Const {
            return Err(RuntimeError::type_error(
                "assignment to an immutable binding",
            ));
        }
        self.values[slot] = Some(value);
        Ok(())
    }
}

struct ScopeFrame<'a> {
    entry: u32,
    plan: &'a ScopeDescriptor,
    environment: Environment,
    binding_hints: Option<Box<[Option<usize>]>>,
}

// Conservative whole-body proof: no captures, calls, property operations,
// arguments access or reference escape. Initial arguments are Number leaves;
// var bindings start undefined and lexical slots retain their TDZ/mutability.
// These instructions produce only primitive values. Neither a private frame's
// intermediate writes nor its per-iteration binding identities can escape.
fn scalar_loop_locals(bytecode: &VerifiedBytecode) -> bool {
    if !bytecode.has_backedge() {
        return false;
    }
    let body = bytecode.bytecode();
    if !matches!(body.instructions.first(), Some(Instruction::EnterScope(index))
        if matches!(&body.constants[*index as usize], Constant::Scope(plan) if plan.kind == ScopeKind::Function))
    {
        return false;
    }
    body.instructions
        .iter()
        .enumerate()
        .all(|(offset, instruction)| match *instruction {
            Instruction::EnterScope(index) => {
                let Constant::Scope(plan) = &body.constants[index as usize] else {
                    return false;
                };
                plan.functions.is_empty()
                    && match plan.kind {
                        ScopeKind::Function => offset == 0,
                        ScopeKind::Block => {
                            plan.bindings.iter().all(|b| b.kind != BindingKind::Var)
                        }
                        ScopeKind::Global => false,
                    }
            }
            Instruction::LoadBinding { .. }
            | Instruction::InitializeBinding { .. }
            | Instruction::StoreBinding { .. }
            | Instruction::UpdateBinding { .. }
            | Instruction::CloneLoopBindings => true,
            Instruction::LoadConstant(index) => matches!(
                body.constants[index as usize],
                Constant::Undefined
                    | Constant::Null
                    | Constant::Boolean(_)
                    | Constant::Number(_)
                    | Constant::String(_)
            ),
            Instruction::Jump(target)
            | Instruction::JumpIfFalse(target)
            | Instruction::JumpIfNullish(target) => target != 0,
            Instruction::ExitScope
            | Instruction::LoadLocal(_)
            | Instruction::StoreLocal(_)
            | Instruction::Duplicate
            | Instruction::Pop
            | Instruction::TypeOf
            | Instruction::UnaryNumeric(_)
            | Instruction::Binary(_)
            | Instruction::Add
            | Instruction::Subtract
            | Instruction::Multiply
            | Instruction::Divide
            | Instruction::StrictEqual
            | Instruction::LogicalNot
            | Instruction::Return
            | Instruction::Complete
            | Instruction::Throw => true,
            _ => false,
        })
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
            let parent = &parents.last().expect("verified block parent").environment;
            // An empty descriptor has no binding identity to capture. Keep its
            // verified scope entry, but reuse the same lookup environment.
            if plan.bindings.is_empty() && plan.functions.is_empty() {
                parent.clone()
            } else {
                let environment = Environment::declarative_from_verified_bindings(
                    parent.clone(),
                    plan.bindings.iter().map(|binding| {
                        (
                            binding.name.clone(),
                            binding.kind != BindingKind::Const,
                            binding.kind == BindingKind::Var,
                        )
                    }),
                );
                instantiate_functions(plan, parents, entry, &environment, realm)?;
                return Ok(environment);
            }
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

#[cfg(test)]
mod private_scope_tests {
    use super::scalar_loop_locals;
    use crate::{ast::StmtKind, bytecode::BytecodeCompiler, parser::parse_script};

    #[test]
    fn lexical_loop_proof_accepts_private_bindings_and_rejects_observers() {
        for (source, eligible) in [
            (
                "function f(n) { let sum=0; for(let i=0;i<n;i++){const x=i; {let i=x+1; sum+=i;}} return sum; }",
                true,
            ),
            (
                "function f(n) { let sum=0; for(let i=0;i<n;i++){sum+=i;} return sum; }",
                true,
            ),
            ("function f(n) { for(let i=0;i<n;i++){let x; x++;} }", true),
            (
                "function f(n) { for(let i=0;i<n;i++){let x=function(){return i;};} }",
                false,
            ),
            ("function f(n) { for(let i=0;i<n;i++){n.value;} }", false),
            ("function f(n) { for(let i=0;i<n;i++){other();} }", false),
            ("function f(n) { for(let i=0;i<n;i++){n=outer;} }", false),
        ] {
            let program = parse_script(source).unwrap();
            let StmtKind::FunctionDeclaration(function) = &program.body[0].kind else {
                panic!("function declaration")
            };
            let template = BytecodeCompiler::compile_function_template(function).unwrap();
            assert_eq!(scalar_loop_locals(&template.body), eligible, "{source}");
        }
    }
}

#[cfg(test)]
mod loop_binding_identity_tests {
    use super::*;
    use crate::{
        ast::StmtKind,
        bytecode::{Bytecode, BytecodeCompiler, ScopeBinding},
        parser::parse_script,
        runtime::{Runtime, ThisBindingStatus},
    };

    #[test]
    fn reuse_excludes_closure_creation_and_eval_observers() {
        for (source, expected) in [
            (
                "function f(n){let total=0;for(let i=0;i<n;i++){total+=outside(i);}return total;}",
                true,
            ),
            (
                "function f(n){let a=[];for(let i=0;i<n;i++){a.push(function(){return i;});}return a;}",
                false,
            ),
            (
                "function f(n){var eval=0;for(let i=0;i<n;i++){outside(i);}return n;}",
                false,
            ),
            (
                "function f(n){for(let i=0;i<n;i++){(0,eval)('x');}return n;}",
                false,
            ),
        ] {
            let program = parse_script(source).unwrap();
            let StmtKind::FunctionDeclaration(function) = &program.body[0].kind else {
                panic!("function")
            };
            let code = BytecodeCompiler::compile_function_template(function).unwrap();
            assert_eq!(code.body.reuse_loop_bindings, expected, "{source}");
        }
    }

    #[test]
    fn references_retained_across_clone_keep_the_original_iteration_cell() {
        let code = Bytecode {
            constants: vec![
                Constant::Scope(ScopeDescriptor {
                    kind: ScopeKind::Function,
                    strict: false,
                    bindings: vec![],
                    functions: vec![],
                }),
                Constant::Scope(ScopeDescriptor {
                    kind: ScopeKind::Block,
                    strict: false,
                    bindings: vec![ScopeBinding {
                        name: "i".into(),
                        kind: BindingKind::Let,
                    }],
                    functions: vec![],
                }),
                Constant::Number(0.0),
                Constant::Number(1.0),
            ],
            instructions: vec![
                Instruction::EnterScope(0),
                Instruction::EnterScope(1),
                Instruction::LoadConstant(2),
                Instruction::InitializeBinding { scope: 1, slot: 0 },
                Instruction::ResolveBinding {
                    scope: 1,
                    slot: 0,
                    strict: false,
                },
                Instruction::CloneLoopBindings,
                Instruction::LoadConstant(3),
                Instruction::StoreBinding { scope: 1, slot: 0 },
                Instruction::LoadReference,
                Instruction::DiscardReference,
                Instruction::Return,
            ],
            local_count: 0,
        }
        .verify()
        .unwrap();
        let realm = Realm::ordinary();
        let frame = Environment::function(
            Some(realm.global_environment()),
            Object::new_in(realm.allocation_context()),
            None,
            ThisBindingStatus::Initialized,
        );
        assert!(matches!(
            VirtualMachine::execute_with_private_frame(&code, &realm, &frame, true).unwrap(),
            Completion::Return(Value::Number(0.0))
        ));
    }

    #[test]
    fn callback_loops_without_captures_allocate_a_constant_number_of_managed_records() {
        let runtime = Runtime::new();
        runtime.set_allocation_collection_interval(usize::MAX);
        let realm = Realm::ordinary_in(&runtime);
        let code=BytecodeCompiler::compile("function increment(x){'use strict';this.value+=x;return this.value;}function bench(n){let counter={value:0,add:increment};for(let i=0;i<n;i++){counter.add(1);}return counter.value;}bench;").unwrap();
        let Completion::Normal(Some(Value::Object(function))) =
            VirtualMachine::execute_in_realm(&code, &realm).unwrap()
        else {
            panic!("function")
        };
        runtime.collect_garbage().unwrap();
        let before = runtime.diagnostics().live;
        assert!(
            function
                .call(Value::Undefined, &[Value::Number(1.0)])
                .unwrap()
                .same_value(&Value::Number(1.0))
        );
        let after_small = runtime.diagnostics().live;
        let count = if cfg!(miri) { 3.0 } else { 50.0 };
        assert!(
            function
                .call(Value::Undefined, &[Value::Number(count)])
                .unwrap()
                .same_value(&Value::Number(count))
        );
        let after_large = runtime.diagnostics().live;
        assert_eq!(after_large - after_small, after_small - before);
        drop((function, realm));
        assert_eq!(runtime.collect_garbage().unwrap().retained, 0);
        assert!(!code.bytecode().instructions.is_empty());
    }
}

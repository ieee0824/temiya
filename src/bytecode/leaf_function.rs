//! Straight-line bodies whose activation bindings cannot be observed.
//! Eligibility is established for the complete body before executing any step.
use super::{
    BinaryOperator, BindingKind, Bytecode, Constant, Instruction, ScopeKind, UnaryNumericOperator,
};
use crate::runtime::{
    JsString, PropertyKey, PropertyReferenceContext, Realm, Reference, ReferencedName,
    RuntimeResult, Value, binary_number, binary_values_in_realm,
};

const LIMIT: usize = 8;

#[derive(Clone, Debug, PartialEq)]
enum Step {
    Literal(Constant),
    Binding(JsString),
    This,
    Get(JsString, bool),
    Resolve(JsString, bool),
    ReadReference,
    StoreReference,
    DiscardReference,
    UpdateReference(bool, bool),
    Binary(BinaryOperator),
    Unary(UnaryNumericOperator),
    TypeOf,
    Not,
    Pop,
    Duplicate,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct LeafFunctionPlan(Vec<Step>);

impl LeafFunctionPlan {
    pub(super) fn compile(code: &Bytecode, max_stack: usize) -> Option<Self> {
        if max_stack > LIMIT {
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
            || scope.bindings.iter().any(|b| b.kind != BindingKind::Var)
        {
            return None;
        }
        let mut steps = Vec::new();
        let mut depth = 0_usize;
        let mut references = 0_usize;
        for (offset, instruction) in code.instructions.iter().enumerate().skip(1) {
            let step = match *instruction {
                Instruction::EnterScope(index) => {
                    let Constant::Scope(block) = code.constants.get(index as usize)? else {
                        return None;
                    };
                    if block.kind != ScopeKind::Block
                        || !block.bindings.is_empty()
                        || !block.functions.is_empty()
                    {
                        return None;
                    }
                    continue;
                }
                Instruction::ExitScope => continue,
                Instruction::LoadConstant(index) => {
                    let value = code.constants.get(index as usize)?;
                    if matches!(
                        value,
                        Constant::Scope(_)
                            | Constant::Function(_)
                            | Constant::Class { .. }
                            | Constant::ObjectLiteral { .. }
                    ) {
                        return None;
                    }
                    depth += 1;
                    Step::Literal(value.clone())
                }
                Instruction::LoadBinding { scope: 0, slot } => {
                    depth += 1;
                    Step::Binding(scope.bindings.get(slot as usize)?.name.clone())
                }
                Instruction::LoadThis => {
                    depth += 1;
                    Step::This
                }
                Instruction::GetNamedProperty { name, strict }
                | Instruction::ResolveNamedProperty { name, strict } => {
                    let Constant::String(name) = code.constants.get(name as usize)? else {
                        return None;
                    };
                    if matches!(instruction, Instruction::ResolveNamedProperty { .. }) {
                        depth = depth.checked_sub(1)?;
                        references += 1;
                        Step::Resolve(name.clone(), strict)
                    } else {
                        Step::Get(name.clone(), strict)
                    }
                }
                Instruction::LoadReference => {
                    if references == 0 {
                        return None;
                    }
                    depth += 1;
                    Step::ReadReference
                }
                Instruction::StoreReference => {
                    references = references.checked_sub(1)?;
                    depth = depth.checked_sub(1)?;
                    Step::StoreReference
                }
                Instruction::DiscardReference => {
                    references = references.checked_sub(1)?;
                    Step::DiscardReference
                }
                Instruction::UpdateReference { increment, prefix } => {
                    references = references.checked_sub(1)?;
                    depth += 1;
                    Step::UpdateReference(increment, prefix)
                }
                Instruction::Add
                | Instruction::Subtract
                | Instruction::Multiply
                | Instruction::Divide
                | Instruction::StrictEqual
                | Instruction::Binary(_) => {
                    let operator = match *instruction {
                        Instruction::Add => BinaryOperator::Add,
                        Instruction::Subtract => BinaryOperator::Subtract,
                        Instruction::Multiply => BinaryOperator::Multiply,
                        Instruction::Divide => BinaryOperator::Divide,
                        Instruction::StrictEqual => BinaryOperator::StrictEqual,
                        Instruction::Binary(op) => op,
                        _ => unreachable!(),
                    };
                    depth = depth.checked_sub(1)?;
                    Step::Binary(operator)
                }
                Instruction::UnaryNumeric(op) => Step::Unary(op),
                Instruction::TypeOf => Step::TypeOf,
                Instruction::LogicalNot => Step::Not,
                Instruction::Pop => {
                    depth = depth.checked_sub(1)?;
                    Step::Pop
                }
                Instruction::Duplicate => {
                    depth += 1;
                    Step::Duplicate
                }
                Instruction::Return
                    if depth == 1 && references == 0 && offset + 1 == code.instructions.len() =>
                {
                    return Some(Self(steps));
                }
                // No captures, binding writes, calls, closures, branches, new.target,
                // arguments object or other operation may expose this activation.
                _ => return None,
            };
            if depth > LIMIT || references > LIMIT {
                return None;
            }
            steps.push(step);
        }
        None
    }

    pub(super) fn execute(
        &self,
        parameters: &[JsString],
        arguments: &[Value],
        this: &Value,
        realm: &Realm,
    ) -> RuntimeResult<Value> {
        let _roots = realm.enter_object_root_region();
        let mut context = PropertyReferenceContext::new(realm);
        let mut stack: [Value; LIMIT] = std::array::from_fn(|_| Value::Undefined);
        let mut refs: [Option<Reference>; LIMIT] = std::array::from_fn(|_| None);
        let mut depth = 0;
        let mut reference_depth = 0;
        fn pop(stack: &mut [Value; LIMIT], depth: &mut usize) -> Value {
            *depth -= 1;
            std::mem::replace(&mut stack[*depth], Value::Undefined)
        }
        for step in &self.0 {
            let pushed = match step {
                Step::Literal(value) => Some(super::vm::constant(value)),
                Step::Binding(name) => Some(
                    parameters
                        .iter()
                        .rposition(|p| p == name)
                        .and_then(|i| arguments.get(i))
                        .cloned()
                        .unwrap_or(Value::Undefined),
                ),
                Step::This => Some(this.clone()),
                Step::Get(name, strict) | Step::Resolve(name, strict) => {
                    let base = pop(&mut stack, &mut depth);
                    let reference = context.reference(
                        base,
                        ReferencedName::PropertyKey(PropertyKey::String(name.clone())),
                        *strict,
                    );
                    if matches!(step, Step::Get(..)) {
                        Some(reference.get_value()?)
                    } else {
                        refs[reference_depth] = Some(reference);
                        reference_depth += 1;
                        None
                    }
                }
                Step::ReadReference => Some(
                    refs[reference_depth - 1]
                        .as_ref()
                        .expect("verified reference")
                        .get_value()?,
                ),
                Step::StoreReference => {
                    let value = pop(&mut stack, &mut depth);
                    reference_depth -= 1;
                    refs[reference_depth]
                        .take()
                        .expect("verified reference")
                        .put_value(value)?;
                    None
                }
                Step::DiscardReference => {
                    reference_depth -= 1;
                    refs[reference_depth] = None;
                    None
                }
                Step::UpdateReference(increment, prefix) => {
                    reference_depth -= 1;
                    let reference = refs[reference_depth].take().expect("verified reference");
                    Some(crate::runtime::update_reference_in_realm(
                        realm, &reference, *increment, *prefix,
                    )?)
                }
                Step::Binary(operator) => {
                    let right = pop(&mut stack, &mut depth);
                    let left = pop(&mut stack, &mut depth);
                    let number = if let (Value::Number(a), Value::Number(b)) = (&left, &right) {
                        binary_number(*operator, *a, *b)
                    } else {
                        None
                    };
                    Some(match number {
                        Some(value) => value,
                        None => binary_values_in_realm(realm, operator.spelling(), left, right)?,
                    })
                }
                Step::Unary(operator) => {
                    let value = pop(&mut stack, &mut depth);
                    Some(crate::runtime::unary_numeric_value(
                        Some(realm),
                        operator.spelling(),
                        value,
                    )?)
                }
                Step::TypeOf => {
                    let value = pop(&mut stack, &mut depth);
                    Some(Value::String(crate::runtime::type_of(&value).into()))
                }
                Step::Not => {
                    let value = pop(&mut stack, &mut depth);
                    Some(Value::Boolean(!value.to_boolean()))
                }
                Step::Pop => {
                    pop(&mut stack, &mut depth);
                    None
                }
                Step::Duplicate => Some(stack[depth - 1].clone()),
            };
            if let Some(value) = pushed {
                stack[depth] = value;
                depth += 1;
            }
        }
        Ok(pop(&mut stack, &mut depth))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ast::StmtKind,
        bytecode::{BytecodeCompiler, VirtualMachine},
        parser::parse_script,
        runtime::{Completion, Runtime},
    };

    #[test]
    fn eligibility_requires_the_entire_body_to_hide_its_activation() {
        for (source, eligible) in [
            (
                "function f(x){'use strict';this.value=x;return this.value;}",
                true,
            ),
            ("function f(x){this.value=x;return this.value;}", true),
            (
                "function f(x){'use strict';this.value=x;let y=1;return y;}",
                false,
            ),
            (
                "function f(x){'use strict';this.value=x;return outside;}",
                false,
            ),
            (
                "function f(x){'use strict';this.value=x;return function(){return x;};}",
                false,
            ),
            (
                "function f(x){'use strict';this.value=x;return arguments[0];}",
                false,
            ),
            (
                "function f(x){'use strict';this.value=x;return other();}",
                false,
            ),
        ] {
            let program = parse_script(source).unwrap();
            let StmtKind::FunctionDeclaration(function) = &program.body[0].kind else {
                panic!("function")
            };
            let template = BytecodeCompiler::compile_function_template(function).unwrap();
            assert_eq!(template.body.leaf_function.is_some(), eligible, "{source}");
        }
    }

    #[test]
    fn property_calls_allocate_no_managed_activation_and_code_holds_no_runtime_edges() {
        let runtime = Runtime::new();
        runtime.set_allocation_collection_interval(usize::MAX);
        let realm = Realm::ordinary_in(&runtime);
        let code = BytecodeCompiler::compile(
            "(function(x){'use strict';this.value=x;return this.value;});",
        )
        .unwrap();
        let Completion::Normal(Some(Value::Object(function))) =
            VirtualMachine::execute_in_realm(&code, &realm).unwrap()
        else {
            panic!("function")
        };
        let receiver = crate::runtime::Object::new_in(realm.allocation_context());
        receiver
            .create_data_property("value".into(), Value::Number(0.0))
            .unwrap();
        runtime.collect_garbage().unwrap();
        let before = runtime.diagnostics();
        for i in 0..if cfg!(miri) { 3 } else { 100 } {
            let result = function
                .call(Value::Object(receiver.clone()), &[Value::Number(i as f64)])
                .unwrap();
            assert!(result.same_value(&Value::Number(i as f64)));
        }
        let after = runtime.diagnostics();
        assert_eq!(after.live, before.live);
        assert_eq!(after.collections, before.collections);
        assert_eq!(after.rooted, before.rooted);
        drop((function, receiver, realm));
        assert_eq!(runtime.collect_garbage().unwrap().retained, 0);
        assert!(!code.bytecode().instructions.is_empty());
    }

    #[test]
    fn reentrant_accessors_keep_receiver_arguments_and_outer_roots_during_collection() {
        let runtime = Runtime::new();
        runtime.set_allocation_collection_interval(1);
        let realm = Realm::ordinary_in(&runtime);
        let make = |source: &str| {
            let code = BytecodeCompiler::compile(source).unwrap();
            let Completion::Normal(Some(Value::Object(function))) =
                VirtualMachine::execute_in_realm(&code, &realm).unwrap()
            else {
                panic!("function")
            };
            (code, function)
        };
        let (code, function) = make("(function(x){'use strict';this.value=x;return this.value;});");
        let (getter_code, getter) = make("(function(){'use strict';return this.saved.value;});");
        let (setter_code, setter) = make("(function(x){'use strict';this.saved={value:x};});");
        let receiver = crate::runtime::Object::new_in(realm.allocation_context());
        let initial = crate::runtime::Object::new_in(realm.allocation_context());
        initial
            .create_data_property("value".into(), Value::Number(0.0))
            .unwrap();
        receiver
            .create_data_property("saved".into(), Value::Object(initial.clone()))
            .unwrap();
        receiver
            .define_own_property(
                "value".into(),
                crate::runtime::PropertyDescriptor::accessor(
                    Some(getter.clone()),
                    Some(setter.clone()),
                    true,
                    true,
                ),
            )
            .unwrap();
        let receiver = Value::Object(receiver);
        let payload = Value::Object(crate::runtime::Object::new_in(realm.allocation_context()));
        let before = runtime.diagnostics().collections;
        for _ in 0..3 {
            assert!(
                function
                    .call(receiver.clone(), std::slice::from_ref(&payload))
                    .unwrap()
                    .same_value(&payload)
            );
        }
        assert!(runtime.diagnostics().collections > before);
        drop((function, receiver, payload, realm, getter, setter, initial));
        assert_eq!(runtime.collect_garbage().unwrap().retained, 0);
        assert!(!code.bytecode().instructions.is_empty());
        assert!(!getter_code.bytecode().instructions.is_empty());
        assert!(!setter_code.bytecode().instructions.is_empty());
    }
}

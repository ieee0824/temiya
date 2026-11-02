//! A call-time guard proving that no parameter expression will execute.
use super::*;

#[derive(Debug)]
pub(super) struct Plan {
    template: Rc<FunctionTemplate>,
    defaults: Box<[usize]>,
    rest: Option<usize>,
}

impl Plan {
    pub(super) fn prepare(function: &Function, kind: FunctionKind) -> Option<Self> {
        if kind != FunctionKind::Normal
            || function.asynchronous
            || function.generator
            || may_observe_arguments(function)
        {
            return None;
        }
        let mut lowered = function.clone();
        let mut defaults = Vec::new();
        let mut rest = None;
        let parameter_count = lowered.parameters.len();
        for (index, parameter) in lowered.parameters.iter_mut().enumerate() {
            match &parameter.kind {
                ExprKind::Identifier(_) => {}
                ExprKind::Assignment { target, .. }
                    if matches!(target.kind, ExprKind::Identifier(_)) =>
                {
                    *parameter = (**target).clone();
                    defaults.push(index);
                }
                ExprKind::Spread(target)
                    if index + 1 == parameter_count
                        && matches!(target.kind, ExprKind::Identifier(_)) =>
                {
                    *parameter = (**target).clone();
                    rest = Some(index);
                }
                _ => return None,
            }
        }
        if defaults.is_empty() && rest.is_none() {
            return None;
        }
        let template = BytecodeCompiler::compile_function_template(&lowered).ok()?;
        if template.arguments_object {
            return None;
        }
        Some(Self {
            template: Rc::new(template),
            defaults: defaults.into_boxed_slice(),
            rest,
        })
    }

    pub(super) fn arguments<'a>(
        &self,
        realm: &Realm,
        arguments: &'a [Value],
    ) -> RuntimeResult<Cow<'a, [Value]>> {
        let Some(start) = self.rest else {
            return Ok(Cow::Borrowed(arguments));
        };
        // No parameter expression ran on an accepted call. Preserve the real,
        // fresh rest-array identity and receiving Realm's intrinsic prototype.
        let array = super::super::array_literal::create(realm)?;
        let mut next = 0;
        for value in arguments.get(start..).unwrap_or_default() {
            super::super::array_literal::append(&array, &mut next, value.clone())?;
        }
        super::super::array_literal::finish(&array, next)?;
        let mut passed = Vec::with_capacity(start + 1);
        passed.extend((0..start).map(|i| arguments.get(i).cloned().unwrap_or(Value::Undefined)));
        passed.push(Value::Object(array));
        Ok(Cow::Owned(passed))
    }

    pub(super) fn for_arguments(&self, arguments: &[Value]) -> Option<Rc<FunctionTemplate>> {
        self.defaults
            .iter()
            .all(|&i| {
                arguments
                    .get(i)
                    .is_some_and(|v| !matches!(v, Value::Undefined))
            })
            .then(|| self.template.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_rest_array_traces_arguments_during_allocation_and_after_return() {
        use crate::{Engine, embedding::ScriptBackend};
        let engine = Engine::builder()
            .script_backend(ScriptBackend::Auto)
            .build();
        {
            let realm = engine.create_realm();
            let script = engine
                .compile(
                    &realm,
                    "function f(value=0,...items){var items;return items;}f;",
                )
                .unwrap();
            let value = engine.evaluate(&script).unwrap();
            let Value::Object(function) = value.value() else {
                panic!("function")
            };
            let object = Object::new_for(function);
            let weak = object.downgrade();
            object
                .create_data_property("self".into(), Value::Object(object.clone()))
                .unwrap();
            engine
                .allocation_context()
                .set_allocation_collection_interval(1);
            let Value::Object(array) = function
                .call(
                    Value::Undefined,
                    &[Value::Number(20.0), Value::Object(object)],
                )
                .unwrap()
            else {
                panic!("rest array")
            };
            engine.collect_garbage().unwrap();
            assert!(weak.upgrade().is_some());
            let Value::Object(element) = array.get(&PropertyKey::integer_index(0)).unwrap() else {
                panic!("argument")
            };
            assert!(
                element
                    .get(&"self".into())
                    .unwrap()
                    .same_value(&Value::Object(element.clone()))
            );
            drop(element);
            drop(array);
            engine.collect_garbage().unwrap();
            assert!(weak.upgrade().is_none());
            engine
                .allocation_context()
                .verify_migration_barriers()
                .unwrap();
        }
        assert_eq!(engine.collect_garbage().unwrap().retained, 0);
    }

    #[test]
    fn guard_is_prepared_only_for_unobservable_skipped_parameter_expressions() {
        let function = |source| {
            let expression = crate::parser::parse_expression(source).unwrap();
            let ExprKind::Function(function) = expression.kind else {
                panic!("function expression")
            };
            *function
        };
        let f = function("function f(a=1,b){var a;return a;}");
        let plan = Plan::prepare(&f, FunctionKind::Normal).unwrap();
        assert!(plan.for_arguments(&[Value::Null]).is_some());
        assert!(plan.for_arguments(&[Value::Boolean(false)]).is_some());
        assert!(plan.for_arguments(&[Value::Undefined]).is_none());
        assert!(plan.for_arguments(&[]).is_none());
        let rest = function("function f(a=1,...rest){return rest;}");
        let plan = Plan::prepare(&rest, FunctionKind::Normal).unwrap();
        assert!(plan.for_arguments(&[Value::Number(42.0)]).is_some());
        assert!(plan.for_arguments(&[Value::Undefined]).is_none());
        for source in [
            "function f(a=1){return arguments[0];}",
            "function f(a=1){return ()=>arguments;}",
            "function f(a=1){eval('a');}",
            "function f({a}={}){return a;}",
            "function f(a=1,...[rest]){return a;}",
            "function* f(a=1){yield a;}",
            "async function f(a=1){return a;}",
        ] {
            assert!(
                Plan::prepare(&function(source), FunctionKind::Normal).is_none(),
                "{source}"
            );
        }
    }
}

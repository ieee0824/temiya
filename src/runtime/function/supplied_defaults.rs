//! Guarded template entry when parameter cells cannot be observed.
use super::*;

#[derive(Debug)]
pub(super) struct Plan {
    template: Rc<FunctionTemplate>,
    defaults: Box<[Default]>,
    rest: Option<usize>,
}

#[derive(Debug)]
struct Default {
    index: usize,
    // A zero-parameter expression arrow can bypass the parameter environment
    // only when its sole name is absent from the entire parameter list.
    outer_arrow: Option<(Rc<Expr>, JsString)>,
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
        let names = function
            .parameters
            .iter()
            .flat_map(crate::semantics::bound_names)
            .map(|name| JsString::from(name.name.as_str()))
            .collect::<Vec<_>>();
        let mut defaults = Vec::new();
        let mut rest = None;
        let parameter_count = lowered.parameters.len();
        for (index, parameter) in lowered.parameters.iter_mut().enumerate() {
            match &parameter.kind {
                ExprKind::Identifier(_) => {}
                ExprKind::Assignment { target, value, .. }
                    if matches!(target.kind, ExprKind::Identifier(_)) =>
                {
                    let outer_arrow = match &value.kind {
                        ExprKind::Arrow(arrow)
                            if lexical_name(arrow, FunctionKind::Arrow).is_some_and(|name| {
                                !names.contains(&name)
                                    && name != JsString::from("arguments")
                                    && name != JsString::from("eval")
                            }) =>
                        {
                            let ExprKind::Identifier(name) = &target.kind else {
                                unreachable!()
                            };
                            Some((Rc::new((**value).clone()), name.as_str().into()))
                        }
                        _ => None,
                    };
                    *parameter = (**target).clone();
                    defaults.push(Default { index, outer_arrow });
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
        closure: &FunctionClosure,
        arguments: &'a [Value],
    ) -> RuntimeResult<Cow<'a, [Value]>> {
        let mut passed = if let Some(start) = self.rest {
            let mut values = Vec::with_capacity(start + 1);
            values.extend(
                (0..start).map(|index| arguments.get(index).cloned().unwrap_or(Value::Undefined)),
            );
            Cow::Owned(values)
        } else {
            Cow::Borrowed(arguments)
        };
        for default in &self.defaults {
            if arguments
                .get(default.index)
                .is_none_or(|v| matches!(v, Value::Undefined))
            {
                let (expression, name) = default
                    .outer_arrow
                    .as_ref()
                    .expect("complete parameter guard precedes initialization");
                // This arrow neither reads nor captures any parameter cell or
                // lexical call context. Named evaluation preserves its fresh
                // identity, inferred name, original source and ordinary lookup.
                let evaluator = ExpressionEvaluator::with_environment(
                    closure.realm.clone(),
                    closure.environment.clone(),
                    Value::Undefined,
                    closure.code.strict,
                )
                .with_function_cache(closure.code.context());
                let value = evaluator.evaluate_named(expression, name)?;
                let values = passed.to_mut();
                if values.len() <= default.index {
                    values.resize(default.index + 1, Value::Undefined);
                }
                values[default.index] = value;
            }
        }
        if let Some(start) = self.rest {
            let array = super::super::array_literal::from_values(
                &closure.realm,
                arguments.get(start..).unwrap_or_default(),
            )?;
            let values = passed.to_mut();
            values.resize(start + 1, Value::Undefined);
            values[start] = Value::Object(array);
        }
        Ok(passed)
    }

    pub(super) fn for_arguments(&self, arguments: &[Value]) -> Option<Rc<FunctionTemplate>> {
        self.defaults
            .iter()
            .all(|default| {
                arguments
                    .get(default.index)
                    .is_some_and(|v| !matches!(v, Value::Undefined))
                    || default.outer_arrow.is_some()
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
        let outer = function("function f(read=()=>outer){var outer=0;return read;}");
        assert!(
            Plan::prepare(&outer, FunctionKind::Normal)
                .unwrap()
                .for_arguments(&[])
                .is_some()
        );
        for source in [
            "function f(read=()=>read){return read;}",
            "function f(read=()=>later,later=1){return read;}",
            "function f(read=()=>rest,...rest){return read;}",
            "function f(read=()=>this){return read;}",
            "function f(read=()=>new.target){return read;}",
            "function f(read=()=>outer,other=effect()){return read;}",
        ] {
            let plan = Plan::prepare(&function(source), FunctionKind::Normal).unwrap();
            assert!(plan.for_arguments(&[]).is_none(), "{source}");
        }
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

    #[test]
    fn outer_default_arrow_traces_enclosing_cells_through_allocation_and_realm_release() {
        use crate::{Engine, embedding::ScriptBackend};
        let engine = Engine::builder()
            .script_backend(ScriptBackend::Auto)
            .build();
        let arrow =
            {
                let realm = engine.create_realm();
                let script = engine.compile(&realm,
                "let outer={answer:42};function f(read=()=>outer){var outer=20;return read;}f;"
            ).unwrap();
                let value = engine.evaluate(&script).unwrap();
                let Value::Object(function) = value.value() else {
                    panic!("function")
                };
                engine
                    .allocation_context()
                    .set_allocation_collection_interval(1);
                let before = engine.allocation_context().diagnostics().collections;
                let Value::Object(arrow) = function.call(Value::Undefined, &[]).unwrap() else {
                    panic!("arrow")
                };
                assert!(engine.allocation_context().diagnostics().collections > before);
                arrow
            };
        engine.collect_garbage().unwrap();
        let Value::Object(object) = arrow.call(Value::Undefined, &[]).unwrap() else {
            panic!("outer object")
        };
        assert!(
            object
                .get(&"answer".into())
                .unwrap()
                .same_value(&Value::Number(42.0))
        );
        engine
            .allocation_context()
            .verify_migration_barriers()
            .unwrap();
        drop(object);
        drop(arrow);
        assert_eq!(engine.collect_garbage().unwrap().retained, 0);
    }
}

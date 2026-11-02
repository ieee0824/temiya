//! A call-time guard proving that no parameter expression will execute.
use super::*;

#[derive(Debug)]
pub(super) struct Plan {
    template: Rc<FunctionTemplate>,
    defaults: Box<[usize]>,
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
        for (index, parameter) in lowered.parameters.iter_mut().enumerate() {
            match &parameter.kind {
                ExprKind::Identifier(_) => {}
                ExprKind::Assignment { target, .. }
                    if matches!(target.kind, ExprKind::Identifier(_)) =>
                {
                    *parameter = (**target).clone();
                    defaults.push(index);
                }
                _ => return None,
            }
        }
        if defaults.is_empty() {
            return None;
        }
        let template = BytecodeCompiler::compile_function_template(&lowered).ok()?;
        if template.arguments_object {
            return None;
        }
        Some(Self {
            template: Rc::new(template),
            defaults: defaults.into_boxed_slice(),
        })
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
        for source in [
            "function f(a=1){return arguments[0];}",
            "function f(a=1){return ()=>arguments;}",
            "function f(a=1){eval('a');}",
            "function f({a}={}){return a;}",
            "function f(a=1,...rest){return a;}",
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

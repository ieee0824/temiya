//! One forward throw guard followed by a return, with unobservable local cells.
use super::{
    BindingKind, Bytecode, Constant, Instruction, ScopeKind, leaf_function::LeafFunctionPlan,
};
use crate::runtime::{Environment, JsString, Object, Realm, RuntimeError, RuntimeResult, Value};

#[derive(Clone, Debug, PartialEq)]
pub(super) struct GuardedLeafPlan {
    condition: LeafFunctionPlan,
    thrown: LeafFunctionPlan,
    returned: LeafFunctionPlan,
}

impl GuardedLeafPlan {
    pub(super) fn compile(code: &Bytecode, max_stack: usize) -> Option<Self> {
        if code.local_count != 0 {
            return None;
        }
        let Instruction::EnterScope(root) = *code.instructions.first()? else {
            return None;
        };
        let Constant::Scope(scope) = code.constants.get(root as usize)? else {
            return None;
        };
        if scope.kind != ScopeKind::Function
            || !scope.functions.is_empty()
            || scope.bindings.iter().any(|b| b.kind != BindingKind::Var)
        {
            return None;
        }
        // Fragment execution initializes the same immutable parameter/var
        // values each time. No local write, capture or exposed Reference may
        // make those independent executions differ from one activation.
        if code.instructions.iter().any(|op| {
            matches!(
                op,
                Instruction::InitializeBinding { .. }
                    | Instruction::StoreBinding { .. }
                    | Instruction::StoreLocal(_)
                    | Instruction::StoreReference
                    | Instruction::UpdateReference { .. }
                    | Instruction::LoadSuperBinding
                    | Instruction::LoadNewTarget
                    | Instruction::CreateFunction(_)
                    | Instruction::CreateClass(_)
            )
        }) {
            return None;
        }
        let mut instructions = Vec::new();
        for (pc, &op) in code.instructions.iter().enumerate() {
            match op {
                Instruction::EnterScope(index) if pc != 0 => {
                    let Constant::Scope(scope) = code.constants.get(index as usize)? else {
                        return None;
                    };
                    if scope.kind != ScopeKind::Block
                        || !scope.bindings.is_empty()
                        || !scope.functions.is_empty()
                    {
                        return None;
                    }
                }
                Instruction::ExitScope => {}
                Instruction::LoadBinding { scope, .. } if scope != 0 => return None,
                _ => instructions.push((pc, op)),
            }
        }
        let branches: Vec<_> = instructions
            .iter()
            .enumerate()
            .filter(|(_, (_, op))| {
                matches!(
                    op,
                    Instruction::Jump(_)
                        | Instruction::JumpIfFalse(_)
                        | Instruction::JumpIfNullish(_)
                )
            })
            .collect();
        if branches.len() != 1 {
            return None;
        }
        let (branch, &(pc, Instruction::JumpIfFalse(target))) = branches[0] else {
            return None;
        };
        if target as usize <= pc {
            return None;
        }
        let target = instructions
            .iter()
            .position(|(pc, _)| *pc >= target as usize)?;
        let condition = Self::fragment(code, &instructions[1..branch], max_stack)?;
        let (_, Instruction::Throw) = *instructions.get(target.checked_sub(1)?)? else {
            return None;
        };
        let thrown = Self::fragment(code, &instructions[branch + 1..target - 1], max_stack)?;
        let (_, Instruction::Return) = *instructions.last()? else {
            return None;
        };
        let returned = Self::fragment(
            code,
            &instructions[target..instructions.len() - 1],
            max_stack,
        )?;
        Some(Self {
            condition,
            thrown,
            returned,
        })
    }

    fn fragment(
        code: &Bytecode,
        input: &[(usize, Instruction)],
        max_stack: usize,
    ) -> Option<LeafFunctionPlan> {
        let mut body = Bytecode {
            constants: code.constants.clone(),
            instructions: vec![*code.instructions.first()?],
            local_count: 0,
        };
        for &(_, op) in input {
            if let Instruction::GetProperty { strict } = op {
                // A primitive literal key has no coercion effects. Fuse only
                // inside this straight-line fragment, never across a target.
                let Instruction::LoadConstant(index) = *body.instructions.last()? else {
                    return None;
                };
                let name = match body.constants.get(index as usize)? {
                    Constant::String(s) => s.clone(),
                    Constant::Number(n)
                        if n.is_finite()
                            && n.fract() == 0.0
                            && *n >= 0.0
                            && *n < u32::MAX as f64 =>
                    {
                        JsString::from((*n as u32).to_string())
                    }
                    _ => return None,
                };
                body.instructions.pop();
                let index = u32::try_from(body.constants.len()).ok()?;
                body.constants.push(Constant::String(name));
                body.instructions.push(Instruction::GetNamedProperty {
                    name: index,
                    strict,
                });
            } else {
                body.instructions.push(op);
            }
        }
        body.instructions.push(Instruction::Return);
        LeafFunctionPlan::compile(&body, max_stack)
    }

    pub(super) fn needs_environment(&self) -> bool {
        self.condition.needs_environment()
            || self.thrown.needs_environment()
            || self.returned.needs_environment()
    }

    pub(super) fn execute(
        &self,
        parameters: &[JsString],
        arguments: &[Value],
        this: &Value,
        realm: &Realm,
        function: &Object,
        environment: Option<&Environment>,
    ) -> RuntimeResult<Value> {
        if self
            .condition
            .execute(parameters, arguments, this, realm, function, environment)?
            .to_boolean()
        {
            let value =
                self.thrown
                    .execute(parameters, arguments, this, realm, function, environment)?;
            Err(RuntimeError::thrown(value))
        } else {
            self.returned
                .execute(parameters, arguments, this, realm, function, environment)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{ast::StmtKind, bytecode::BytecodeCompiler, parser::parse_script};

    #[test]
    fn eligibility_checks_all_fragments_before_any_execution() {
        for (source, eligible) in [
            (
                "function f(value,rest){var value;if(rest[0]!==value)throw 42;return value;}",
                true,
            ),
            (
                "function f(x){if(x.flag){throw x.error;}return x.result;}",
                true,
            ),
            ("function f(x){if(x['flag'])throw 7;return outside;}", true),
            ("function f(x){var y=x;if(y)throw 7;return y;}", false),
            ("function f(x){if(x.flag())throw 7;return x;}", false),
            ("function f(x){if(x.flag)throw other();return x;}", false),
            ("function f(x){if(x.flag)throw 7;return other();}", false),
            ("function f(x){if(x.flag)throw 7;return ()=>x;}", false),
            (
                "function f(x){if(x.flag){let y=42;throw y;}return x;}",
                false,
            ),
            ("function f(x){while(x){}if(x)throw 7;return x;}", false),
            (
                "function f(x){if(x)throw 7;if(outside)throw 8;return x;}",
                false,
            ),
            ("function f(x){if(x)throw 7;return arguments;}", false),
            ("function f(x){if(x)throw eval;return x;}", false),
            ("function f(x,k){if(x[k])throw 7;return x;}", false),
        ] {
            let program = parse_script(source).unwrap();
            let StmtKind::FunctionDeclaration(function) = &program.body[0].kind else {
                panic!("function declaration")
            };
            let template = BytecodeCompiler::compile_function_template(function).unwrap();
            assert!(template.body.leaf_function.is_none(), "{source}");
            assert_eq!(template.body.guarded_leaf.is_some(), eligible, "{source}");
        }
    }
}

//! Resumable generator objects and their state machine.

use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use crate::ast::{Expr, ExprKind, FunctionBody, Stmt, StmtKind, VariableKind};

use super::{
    Completion, Environment, ExpressionEvaluator, FunctionKind, JsString, Object, PropertyKey,
    RuntimeError, RuntimeResult, StatementEvaluator, ThisBindingStatus, Value,
    function::{FunctionClosure, bind_arguments, bind_parameters, ordinary_this},
};

/// Observable lifecycle of a generator instance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeneratorState {
    /// Created but not yet entered.
    SuspendedStart,
    /// Paused at a `yield` expression.
    SuspendedYield,
    /// Currently executing; recursive resumption is rejected.
    Executing,
    /// Finished normally, by return, or by abrupt resumption.
    Completed,
}

/// The iterator result returned by generator resumption.
#[derive(Clone, Debug)]
pub struct IteratorResult {
    /// Yielded or returned value.
    pub value: Value,
    /// Whether the generator has completed.
    pub done: bool,
}

#[derive(Clone)]
enum Continuation {
    Ignore,
    Bind { name: JsString, initialize: bool },
    Assign(Expr),
    AssignReference(super::Reference),
    Return,
}

struct GeneratorData {
    state: GeneratorState,
    evaluator: ExpressionEvaluator,
    statements: StatementEvaluator,
    body: Vec<Stmt>,
    pc: usize,
    continuation: Option<Continuation>,
    delegated: VecDeque<Value>,
    delegating: bool,
    returned: Option<Value>,
}

/// A resumable ECMAScript generator instance.
#[derive(Clone)]
pub struct Generator(Rc<RefCell<GeneratorData>>);

impl std::fmt::Debug for Generator {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Generator")
            .field("state", &self.state())
            .finish_non_exhaustive()
    }
}

impl Generator {
    /// Returns the current lifecycle state.
    #[must_use]
    pub fn state(&self) -> GeneratorState {
        self.0.borrow().state
    }

    /// Resumes normally, supplying the result of the suspended `yield`.
    pub fn next(&self, value: Value) -> RuntimeResult<IteratorResult> {
        self.resume(Resume::Next(value))
    }

    /// Abruptly resumes by throwing a value at the suspension point.
    pub fn throw(&self, value: Value) -> RuntimeResult<IteratorResult> {
        self.resume(Resume::Throw(value))
    }

    /// Abruptly resumes with a return completion.
    pub fn return_(&self, value: Value) -> RuntimeResult<IteratorResult> {
        self.resume(Resume::Return(value))
    }

    fn resume(&self, resume: Resume) -> RuntimeResult<IteratorResult> {
        let mut data = self.0.borrow_mut();
        match data.state {
            GeneratorState::Executing => {
                return Err(RuntimeError::type_error("generator is already executing"));
            }
            GeneratorState::Completed => {
                return match resume {
                    Resume::Throw(value) => Err(RuntimeError::thrown(value)),
                    Resume::Next(_) => Ok(IteratorResult {
                        value: Value::Undefined,
                        done: true,
                    }),
                    Resume::Return(value) => Ok(IteratorResult { value, done: true }),
                };
            }
            GeneratorState::SuspendedStart | GeneratorState::SuspendedYield => {}
        }
        if matches!(resume, Resume::Throw(_)) {
            data.state = GeneratorState::Completed;
            let Resume::Throw(value) = resume else {
                unreachable!()
            };
            return Err(RuntimeError::thrown(value));
        }
        if let Resume::Return(value) = resume {
            data.state = GeneratorState::Completed;
            return Ok(IteratorResult { value, done: true });
        }
        let Resume::Next(input) = resume else {
            unreachable!()
        };
        if data.state == GeneratorState::SuspendedYield && !data.delegating {
            apply_continuation(&mut data, input)?;
            if let Some(value) = data.returned.take() {
                return Ok(IteratorResult { value, done: true });
            }
        }
        data.state = GeneratorState::Executing;
        match execute(&mut data) {
            Ok(result) => Ok(result),
            Err(error) => {
                data.state = GeneratorState::Completed;
                Err(error)
            }
        }
    }
}

enum Resume {
    Next(Value),
    Throw(Value),
    Return(Value),
}

pub(super) fn create_generator_object(
    closure: &FunctionClosure,
    function: Object,
    this_argument: Value,
    arguments: &[Value],
) -> RuntimeResult<Object> {
    let lexical = closure.kind == FunctionKind::Arrow;
    let this_value = if lexical {
        closure.lexical_this.clone()
    } else if closure.code.strict {
        this_argument
    } else {
        ordinary_this(&closure.realm, this_argument)?
    };
    let environment = Environment::function(
        Some(closure.environment.clone()),
        function,
        None,
        if lexical {
            ThisBindingStatus::Lexical
        } else {
            ThisBindingStatus::Uninitialized
        },
    );
    if !lexical {
        environment.bind_this_value(this_value.clone())?;
        bind_arguments(&environment, arguments)?;
    }
    let evaluator = ExpressionEvaluator::with_environment(
        closure.realm.clone(),
        environment.clone(),
        this_value.clone(),
        closure.code.strict,
    );
    bind_parameters(
        &evaluator,
        &environment,
        &closure.code.parameters,
        arguments,
    )?;
    let FunctionBody::Block(body) = &closure.code.body else {
        return Err(RuntimeError::syntax_error(
            "generator requires a block body",
        ));
    };
    let StmtKind::Block(body) = &body.kind else {
        return Err(RuntimeError::internal_error(
            "generator body is not represented as a block",
        ));
    };
    instantiate_bindings(&environment, body)?;
    let statements = StatementEvaluator::with_environments(
        closure.realm.clone(),
        environment.clone(),
        environment,
        this_value,
        closure.code.strict,
    );
    let generator = Generator(Rc::new(RefCell::new(GeneratorData {
        state: GeneratorState::SuspendedStart,
        evaluator,
        statements,
        body: body.clone(),
        pc: 0,
        continuation: None,
        delegated: VecDeque::new(),
        delegating: false,
        returned: None,
    })));
    generator_object(generator)
}

fn generator_object(generator: Generator) -> RuntimeResult<Object> {
    let object = Object::new();
    let next = generator.clone();
    object.create_data_property(
        "next".into(),
        Value::Object(Object::native_function(move |_, arguments| {
            iterator_result_object(
                next.next(arguments.first().cloned().unwrap_or(Value::Undefined))?,
            )
            .map(Value::Object)
        })),
    )?;
    let throw = generator.clone();
    object.create_data_property(
        "throw".into(),
        Value::Object(Object::native_function(move |_, arguments| {
            iterator_result_object(
                throw.throw(arguments.first().cloned().unwrap_or(Value::Undefined))?,
            )
            .map(Value::Object)
        })),
    )?;
    object.create_data_property(
        "return".into(),
        Value::Object(Object::native_function(move |_, arguments| {
            iterator_result_object(
                generator.return_(arguments.first().cloned().unwrap_or(Value::Undefined))?,
            )
            .map(Value::Object)
        })),
    )?;
    Ok(object)
}

fn iterator_result_object(result: IteratorResult) -> RuntimeResult<Object> {
    let object = Object::new();
    object.create_data_property("value".into(), result.value)?;
    object.create_data_property("done".into(), Value::Boolean(result.done))?;
    Ok(object)
}

fn execute(data: &mut GeneratorData) -> RuntimeResult<IteratorResult> {
    if data.delegating {
        if let Some(value) = data.delegated.pop_front() {
            data.state = GeneratorState::SuspendedYield;
            return Ok(IteratorResult { value, done: false });
        }
        data.delegating = false;
        apply_continuation(data, Value::Undefined)?;
        if let Some(value) = data.returned.take() {
            return Ok(IteratorResult { value, done: true });
        }
    }
    while data.pc < data.body.len() {
        let statement = data.body[data.pc].clone();
        data.pc += 1;
        if expand_control_flow(data, &statement)? {
            continue;
        }
        if let Some((argument, delegate, continuation)) = yield_statement(&statement) {
            let value = argument.as_ref().map_or(Ok(Value::Undefined), |argument| {
                data.evaluator.evaluate(argument)
            })?;
            let continuation = match continuation {
                Continuation::Assign(target) => {
                    Continuation::AssignReference(data.evaluator.evaluate_reference(&target)?)
                }
                continuation => continuation,
            };
            if delegate {
                data.delegated = iterable_values(&value)?.into();
                data.continuation = Some(continuation);
                data.delegating = true;
                if let Some(value) = data.delegated.pop_front() {
                    data.state = GeneratorState::SuspendedYield;
                    return Ok(IteratorResult { value, done: false });
                }
                data.delegating = false;
                apply_continuation(data, Value::Undefined)?;
                if let Some(value) = data.returned.take() {
                    return Ok(IteratorResult { value, done: true });
                }
                continue;
            }
            data.continuation = Some(continuation);
            data.state = GeneratorState::SuspendedYield;
            return Ok(IteratorResult { value, done: false });
        }
        match data.statements.evaluate_statement(&statement)? {
            Completion::Normal(_) => {}
            Completion::Return(value) => {
                data.state = GeneratorState::Completed;
                return Ok(IteratorResult { value, done: true });
            }
            Completion::Throw(value) => {
                data.state = GeneratorState::Completed;
                return Err(RuntimeError::thrown(value));
            }
            Completion::Break { .. } | Completion::Continue { .. } => {
                return Err(RuntimeError::internal_error(
                    "unsupported generator control flow escaped its body",
                ));
            }
        }
    }
    data.state = GeneratorState::Completed;
    Ok(IteratorResult {
        value: Value::Undefined,
        done: true,
    })
}

fn expand_control_flow(data: &mut GeneratorData, statement: &Stmt) -> RuntimeResult<bool> {
    if !contains_yield(statement) {
        return Ok(false);
    }
    match &statement.kind {
        StmtKind::Block(statements) => {
            data.body
                .splice(data.pc..data.pc, statements.iter().cloned());
            Ok(true)
        }
        StmtKind::If {
            test,
            consequent,
            alternate,
        } => {
            let selected = if data.evaluator.evaluate(test)?.to_boolean() {
                Some((**consequent).clone())
            } else {
                alternate.as_deref().cloned()
            };
            if let Some(selected) = selected {
                data.body.insert(data.pc, selected);
            }
            Ok(true)
        }
        StmtKind::While { test, body } => {
            if data.evaluator.evaluate(test)?.to_boolean() {
                data.body.insert(data.pc, statement.clone());
                data.body.insert(data.pc, (**body).clone());
            }
            Ok(true)
        }
        StmtKind::DoWhile { body, test } => {
            data.body.insert(
                data.pc,
                Stmt {
                    span: statement.span,
                    kind: StmtKind::While {
                        test: test.clone(),
                        body: body.clone(),
                    },
                },
            );
            data.body.insert(data.pc, (**body).clone());
            Ok(true)
        }
        StmtKind::For {
            init,
            test,
            update,
            body,
        } => {
            if let Some(init) = init {
                let kind = match init {
                    crate::ast::ForInit::Expression(expression) => {
                        StmtKind::Expression(expression.clone())
                    }
                    crate::ast::ForInit::Variable { kind, declarations } => StmtKind::Variable {
                        kind: *kind,
                        declarations: declarations.clone(),
                    },
                };
                data.statements.evaluate_statement(&Stmt {
                    span: statement.span,
                    kind,
                })?;
            }
            let mut iteration = vec![(**body).clone()];
            if let Some(update) = update {
                iteration.push(Stmt {
                    span: update.span,
                    kind: StmtKind::Expression(update.clone()),
                });
            }
            let test = test.clone().unwrap_or(Expr {
                span: statement.span,
                kind: ExprKind::Literal(crate::ast::Literal::Boolean(true)),
            });
            data.body.insert(
                data.pc,
                Stmt {
                    span: statement.span,
                    kind: StmtKind::While {
                        test,
                        body: Box::new(Stmt {
                            span: statement.span,
                            kind: StmtKind::Block(iteration),
                        }),
                    },
                },
            );
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn contains_yield(statement: &Stmt) -> bool {
    fn expression_has_yield(expression: &Expr) -> bool {
        match &expression.kind {
            ExprKind::Yield { .. } => true,
            ExprKind::Parenthesized(inner) | ExprKind::Spread(inner) => expression_has_yield(inner),
            ExprKind::Unary { argument, .. } | ExprKind::Update { argument, .. } => {
                expression_has_yield(argument)
            }
            ExprKind::Binary { left, right, .. } => {
                expression_has_yield(left) || expression_has_yield(right)
            }
            ExprKind::Conditional {
                test,
                consequent,
                alternate,
            } => {
                expression_has_yield(test)
                    || expression_has_yield(consequent)
                    || expression_has_yield(alternate)
            }
            ExprKind::Assignment { target, value, .. } => {
                expression_has_yield(target) || expression_has_yield(value)
            }
            ExprKind::Sequence(items) => items.iter().any(expression_has_yield),
            _ => false,
        }
    }
    match &statement.kind {
        StmtKind::Expression(value) | StmtKind::Throw(value) => expression_has_yield(value),
        StmtKind::Return(value) => value.as_ref().is_some_and(expression_has_yield),
        StmtKind::Block(body) => body.iter().any(contains_yield),
        StmtKind::If {
            consequent,
            alternate,
            ..
        } => contains_yield(consequent) || alternate.as_deref().is_some_and(contains_yield),
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::For { body, .. } => contains_yield(body),
        StmtKind::Variable { declarations, .. } => declarations
            .iter()
            .any(|declaration| declaration.init.as_ref().is_some_and(expression_has_yield)),
        _ => false,
    }
}

fn yield_statement(statement: &Stmt) -> Option<(Option<Expr>, bool, Continuation)> {
    match &statement.kind {
        StmtKind::Expression(Expr {
            kind: ExprKind::Yield { argument, delegate },
            ..
        }) => Some((
            argument.as_deref().cloned(),
            *delegate,
            Continuation::Ignore,
        )),
        StmtKind::Return(Some(Expr {
            kind: ExprKind::Yield { argument, delegate },
            ..
        })) => Some((
            argument.as_deref().cloned(),
            *delegate,
            Continuation::Return,
        )),
        StmtKind::Expression(Expr {
            kind: ExprKind::Assignment { target, value, .. },
            ..
        }) if matches!(value.kind, ExprKind::Yield { .. }) => {
            let ExprKind::Yield { argument, delegate } = &value.kind else {
                unreachable!()
            };
            Some((
                argument.as_deref().cloned(),
                *delegate,
                Continuation::Assign((**target).clone()),
            ))
        }
        StmtKind::Variable { kind, declarations } if declarations.len() == 1 => {
            let declaration = &declarations[0];
            let ExprKind::Identifier(name) = &declaration.binding.kind else {
                return None;
            };
            let Some(Expr {
                kind: ExprKind::Yield { argument, delegate },
                ..
            }) = &declaration.init
            else {
                return None;
            };
            Some((
                argument.as_deref().cloned(),
                *delegate,
                Continuation::Bind {
                    name: JsString::from(name.as_str()),
                    initialize: *kind != VariableKind::Var,
                },
            ))
        }
        _ => None,
    }
}

fn apply_continuation(data: &mut GeneratorData, value: Value) -> RuntimeResult<()> {
    let Some(continuation) = data.continuation.take() else {
        return Ok(());
    };
    match continuation {
        Continuation::Ignore => Ok(()),
        Continuation::Assign(_) => unreachable!("reference is resolved before suspension"),
        Continuation::AssignReference(reference) => reference.put_value(value),
        Continuation::Return => {
            data.state = GeneratorState::Completed;
            data.returned = Some(value);
            Ok(())
        }
        Continuation::Bind { name, initialize } => {
            if initialize {
                data.evaluator.environment.initialize_binding(&name, value)
            } else {
                data.evaluator
                    .environment
                    .set_mutable_binding(&name, value, false)
            }
        }
    }
}

fn instantiate_bindings(environment: &Environment, body: &[Stmt]) -> RuntimeResult<()> {
    for statement in body {
        match &statement.kind {
            StmtKind::Variable { kind, declarations } => {
                instantiate_declarations(environment, *kind, declarations)?;
            }
            StmtKind::For {
                init: Some(crate::ast::ForInit::Variable { kind, declarations }),
                body,
                ..
            } => {
                instantiate_declarations(environment, *kind, declarations)?;
                instantiate_bindings(environment, std::slice::from_ref(body))?;
            }
            StmtKind::Block(statements) => instantiate_bindings(environment, statements)?,
            _ => {}
        }
    }
    Ok(())
}

fn instantiate_declarations(
    environment: &Environment,
    kind: VariableKind,
    declarations: &[crate::ast::VariableDeclarator],
) -> RuntimeResult<()> {
    for declaration in declarations {
        let ExprKind::Identifier(name) = &declaration.binding.kind else {
            return Err(RuntimeError::internal_error(
                "destructuring generator bindings are not implemented",
            ));
        };
        let name = JsString::from(name.as_str());
        if environment.has_binding(&name)? {
            continue;
        }
        if kind == VariableKind::Const {
            environment.create_immutable_binding(name, true)?;
        } else {
            environment.create_mutable_binding(name.clone(), false)?;
            if kind == VariableKind::Var {
                environment.initialize_binding(&name, Value::Undefined)?;
            }
        }
    }
    Ok(())
}

fn iterable_values(value: &Value) -> RuntimeResult<Vec<Value>> {
    match value {
        Value::String(string) => Ok(string
            .code_units()
            .iter()
            .map(|unit| Value::String(JsString::from_code_units(vec![*unit])))
            .collect()),
        Value::Object(object) => {
            let length = object.get(&PropertyKey::from("length"))?.to_number()?;
            let length = if length.is_finite() && length > 0.0 {
                length.min(u32::MAX as f64) as u32
            } else {
                0
            };
            (0..length)
                .map(|index| object.get(&PropertyKey::String(JsString::from(index.to_string()))))
                .collect()
        }
        _ => Err(RuntimeError::type_error("yield* operand is not iterable")),
    }
}

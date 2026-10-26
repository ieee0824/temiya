//! Statement evaluation and ECMAScript Completion Records.

use crate::ast::{
    CatchClause, Expr, ExprKind, ForInit, Program, Stmt, StmtKind, SwitchCase, VariableDeclarator,
    VariableKind,
};

use super::{
    Environment, ExpressionEvaluator, FunctionKind, JsString, ObjectEnvironmentOptions,
    PropertyKey, Realm, RuntimeError, RuntimeResult, Value, function::create_function,
    interpreter::strict_equal,
};

/// The completion produced by statement evaluation.
#[derive(Clone, Debug)]
pub enum Completion {
    /// Ordinary completion. `None` represents an empty completion value.
    Normal(Option<Value>),
    /// A break completion with its optional target label.
    Break {
        /// Value accumulated before the break.
        value: Option<Value>,
        /// Target label, or `None` for the nearest breakable statement.
        target: Option<String>,
    },
    /// A continue completion with its optional target label.
    Continue {
        /// Value accumulated before the continue.
        value: Option<Value>,
        /// Target label, or `None` for the nearest loop.
        target: Option<String>,
    },
    /// A function return completion.
    Return(Value),
    /// A JavaScript throw completion.
    Throw(Value),
}

impl Completion {
    /// Returns whether this completion is ordinary rather than abrupt.
    #[must_use]
    pub fn is_normal(&self) -> bool {
        matches!(self, Self::Normal(_))
    }

    /// Returns the completion's value, when present.
    #[must_use]
    pub fn value(&self) -> Option<&Value> {
        match self {
            Self::Normal(value) | Self::Break { value, .. } | Self::Continue { value, .. } => {
                value.as_ref()
            }
            Self::Return(value) | Self::Throw(value) => Some(value),
        }
    }

    fn update_empty(self, value: &Option<Value>) -> Self {
        match self {
            Self::Normal(None) => Self::Normal(value.clone()),
            Self::Break {
                value: None,
                target,
            } => Self::Break {
                value: value.clone(),
                target,
            },
            Self::Continue {
                value: None,
                target,
            } => Self::Continue {
                value: value.clone(),
                target,
            },
            completion => completion,
        }
    }
}

/// Direct evaluator for programs and statements using Completion Records.
#[derive(Clone, Debug)]
pub struct StatementEvaluator {
    expression: ExpressionEvaluator,
    variable_environment: Environment,
}

impl StatementEvaluator {
    /// Creates an evaluator in a Realm's global environment.
    #[must_use]
    pub fn new(realm: Realm) -> Self {
        let environment = realm.global_environment();
        Self {
            expression: ExpressionEvaluator::new(realm),
            variable_environment: environment,
        }
    }

    /// Creates an evaluator with explicit lexical and variable environments.
    #[must_use]
    pub fn with_environments(
        realm: Realm,
        lexical_environment: Environment,
        variable_environment: Environment,
        this_value: Value,
        strict: bool,
    ) -> Self {
        Self {
            expression: ExpressionEvaluator::with_environment(
                realm,
                lexical_environment,
                this_value,
                strict,
            ),
            variable_environment,
        }
    }

    /// Evaluates a parsed program after declaration instantiation.
    pub fn evaluate_program(&self, program: &Program) -> RuntimeResult<Completion> {
        let mut evaluator = self.clone();
        evaluator.expression.strict |= program.strict;
        evaluator.instantiate_var_statements(&program.body)?;
        evaluator.instantiate_lexical(&program.body, &evaluator.expression.environment)?;
        evaluator.instantiate_functions(&program.body)?;
        evaluator.statement_list(&program.body)
    }

    /// Evaluates one statement.
    pub fn evaluate_statement(&self, statement: &Stmt) -> RuntimeResult<Completion> {
        self.instantiate_var_statements(std::slice::from_ref(statement))?;
        self.statement(statement, &[])
    }

    pub(super) fn evaluate_instantiated(&self, statements: &[Stmt]) -> RuntimeResult<Completion> {
        self.instantiate_functions(statements)?;
        self.statement_list(statements)
    }

    fn nested(&self, environment: Environment) -> Self {
        let mut expression = self.expression.clone();
        expression.environment = environment;
        Self {
            expression,
            variable_environment: self.variable_environment.clone(),
        }
    }

    fn statement_list(&self, statements: &[Stmt]) -> RuntimeResult<Completion> {
        let mut value = None;
        for statement in statements {
            let completion = self.statement(statement, &[])?.update_empty(&value);
            match completion {
                Completion::Normal(next) => value = next,
                abrupt => return Ok(abrupt),
            }
        }
        Ok(Completion::Normal(value))
    }

    fn statement(&self, statement: &Stmt, labels: &[String]) -> RuntimeResult<Completion> {
        match &statement.kind {
            StmtKind::Empty | StmtKind::Debugger => Ok(Completion::Normal(None)),
            StmtKind::Expression(expression) => self
                .expression
                .evaluate(expression)
                .map(|value| Completion::Normal(Some(value))),
            StmtKind::Block(statements) => self.block(statements),
            StmtKind::If {
                test,
                consequent,
                alternate,
            } => {
                if self.expression.evaluate(test)?.to_boolean() {
                    self.statement(consequent, &[])
                } else if let Some(alternate) = alternate {
                    self.statement(alternate, &[])
                } else {
                    Ok(Completion::Normal(None))
                }
            }
            StmtKind::While { test, body } => self.while_loop(test, body, labels, false),
            StmtKind::DoWhile { test, body } => self.while_loop(test, body, labels, true),
            StmtKind::For {
                init,
                test,
                update,
                body,
            } => {
                if matches!(init, Some(ForInit::Variable { kind, .. }) if *kind != VariableKind::Var)
                {
                    let environment =
                        Environment::declarative(Some(self.expression.environment.clone()));
                    self.nested(environment).for_loop(
                        init.as_ref(),
                        test.as_ref(),
                        update.as_ref(),
                        body,
                        labels,
                    )
                } else {
                    self.for_loop(init.as_ref(), test.as_ref(), update.as_ref(), body, labels)
                }
            }
            StmtKind::ForEach {
                left,
                right,
                of,
                asynchronous,
                body,
            } => {
                if *asynchronous {
                    Err(RuntimeError::internal_error(
                        "for-await-of is implemented by milestone #28",
                    ))
                } else {
                    self.for_each(left, right, *of, body, labels)
                }
            }
            StmtKind::Labeled { label, body } => {
                let mut nested_labels = labels.to_vec();
                nested_labels.push(label.clone());
                let completion = self.statement(body, &nested_labels)?;
                match completion {
                    Completion::Break { value, target } if target.as_deref() == Some(label) => {
                        Ok(Completion::Normal(value))
                    }
                    completion => Ok(completion),
                }
            }
            StmtKind::Break(target) => Ok(Completion::Break {
                value: None,
                target: target.clone(),
            }),
            StmtKind::Continue(target) => Ok(Completion::Continue {
                value: None,
                target: target.clone(),
            }),
            StmtKind::Return(expression) => Ok(Completion::Return(
                expression.as_ref().map_or(Ok(Value::Undefined), |value| {
                    self.expression.evaluate(value)
                })?,
            )),
            StmtKind::Throw(expression) => {
                Ok(Completion::Throw(self.expression.evaluate(expression)?))
            }
            StmtKind::Switch {
                discriminant,
                cases,
            } => {
                let environment =
                    Environment::declarative(Some(self.expression.environment.clone()));
                for case in cases {
                    self.instantiate_lexical(&case.consequent, &environment)?;
                }
                self.nested(environment).switch(discriminant, cases)
            }
            StmtKind::Try {
                block,
                handler,
                finalizer,
            } => self.try_statement(block, handler.as_ref(), finalizer.as_deref()),
            StmtKind::With { object, body } => {
                let object = match self.expression.evaluate(object)? {
                    Value::Object(object) => object,
                    Value::Undefined | Value::Null => {
                        return Err(RuntimeError::type_error(
                            "with value cannot be null or undefined",
                        ));
                    }
                    Value::String(string) => {
                        let object = super::Object::new();
                        object.create_data_property(
                            PropertyKey::from("length"),
                            Value::Number(string.code_units().len() as f64),
                        )?;
                        for (index, unit) in string.code_units().iter().enumerate() {
                            object.create_data_property(
                                PropertyKey::String(JsString::from(index.to_string())),
                                Value::String(JsString::from_code_units(vec![*unit])),
                            )?;
                        }
                        object
                    }
                    _ => super::Object::new(),
                };
                let environment = Environment::object(
                    object,
                    Some(self.expression.environment.clone()),
                    ObjectEnvironmentOptions {
                        with_environment: true,
                        unscopables_key: None,
                    },
                );
                self.nested(environment).statement(body, &[])
            }
            StmtKind::Variable { kind, declarations } => {
                self.evaluate_declarations(*kind, declarations)
            }
            StmtKind::FunctionDeclaration(_) => Ok(Completion::Normal(None)),
            StmtKind::ClassDeclaration(_)
            | StmtKind::ImportDeclaration(_)
            | StmtKind::ExportDeclaration(_) => Err(RuntimeError::internal_error(
                "function, class, and module declaration execution belongs to later milestones",
            )),
        }
    }

    fn block(&self, statements: &[Stmt]) -> RuntimeResult<Completion> {
        let environment = Environment::declarative(Some(self.expression.environment.clone()));
        self.instantiate_lexical(statements, &environment)?;
        let evaluator = self.nested(environment);
        evaluator.instantiate_functions(statements)?;
        evaluator.statement_list(statements)
    }

    fn while_loop(
        &self,
        test: &Expr,
        body: &Stmt,
        labels: &[String],
        first: bool,
    ) -> RuntimeResult<Completion> {
        let mut value = None;
        let mut initial = first;
        while initial || self.expression.evaluate(test)?.to_boolean() {
            initial = false;
            match self.statement(body, &[])?.update_empty(&value) {
                Completion::Normal(next) => value = next,
                Completion::Continue {
                    value: next,
                    target,
                } if target.is_none() || target.as_ref().is_some_and(|x| labels.contains(x)) => {
                    value = next;
                }
                Completion::Break {
                    value: next,
                    target,
                } if target.is_none() || target.as_ref().is_some_and(|x| labels.contains(x)) => {
                    return Ok(Completion::Normal(next));
                }
                abrupt => return Ok(abrupt),
            }
        }
        Ok(Completion::Normal(value))
    }

    fn for_loop(
        &self,
        init: Option<&ForInit>,
        test: Option<&Expr>,
        update: Option<&Expr>,
        body: &Stmt,
        labels: &[String],
    ) -> RuntimeResult<Completion> {
        if let Some(init) = init {
            self.evaluate_for_init(init)?;
        }
        let mut value = None;
        loop {
            if let Some(test) = test {
                if !self.expression.evaluate(test)?.to_boolean() {
                    break;
                }
            }
            match self.statement(body, &[])?.update_empty(&value) {
                Completion::Normal(next) => value = next,
                Completion::Continue {
                    value: next,
                    target,
                } if target.is_none() || target.as_ref().is_some_and(|x| labels.contains(x)) => {
                    value = next;
                }
                Completion::Break {
                    value: next,
                    target,
                } if target.is_none() || target.as_ref().is_some_and(|x| labels.contains(x)) => {
                    return Ok(Completion::Normal(next));
                }
                abrupt => return Ok(abrupt),
            }
            if let Some(update) = update {
                self.expression.evaluate(update)?;
            }
        }
        Ok(Completion::Normal(value))
    }

    fn for_each(
        &self,
        left: &ForInit,
        right: &Expr,
        of: bool,
        body: &Stmt,
        labels: &[String],
    ) -> RuntimeResult<Completion> {
        let right = self.expression.evaluate(right)?;
        let values = if of {
            iterable_values(&right)?
        } else {
            enumerable_keys(&right)?
        };
        let mut result = None;
        for value in values {
            let completion = match left {
                ForInit::Variable { kind, declarations } if *kind != VariableKind::Var => {
                    let declaration = declarations
                        .first()
                        .ok_or_else(|| RuntimeError::syntax_error("missing for-each binding"))?;
                    let name = binding_name(&declaration.binding)?;
                    let environment =
                        Environment::declarative(Some(self.expression.environment.clone()));
                    if *kind == VariableKind::Const {
                        environment.create_immutable_binding(name.clone(), true)?;
                    } else {
                        environment.create_mutable_binding(name.clone(), false)?;
                    }
                    environment.initialize_binding(&name, value)?;
                    self.nested(environment).statement(body, &[])?
                }
                _ => {
                    self.assign_for_init(left, value)?;
                    self.statement(body, &[])?
                }
            };
            match completion.update_empty(&result) {
                Completion::Normal(next) => result = next,
                Completion::Continue {
                    value: next,
                    target,
                } if target.is_none() || target.as_ref().is_some_and(|x| labels.contains(x)) => {
                    result = next;
                }
                Completion::Break {
                    value: next,
                    target,
                } if target.is_none() || target.as_ref().is_some_and(|x| labels.contains(x)) => {
                    return Ok(Completion::Normal(next));
                }
                abrupt => return Ok(abrupt),
            }
        }
        Ok(Completion::Normal(result))
    }

    fn switch(&self, discriminant: &Expr, cases: &[SwitchCase]) -> RuntimeResult<Completion> {
        let input = self.expression.evaluate(discriminant)?;
        let mut start = None;
        let mut default = None;
        for (index, case) in cases.iter().enumerate() {
            if let Some(test) = &case.test {
                if start.is_none() && strict_equal(&input, &self.expression.evaluate(test)?) {
                    start = Some(index);
                }
            } else {
                default = Some(index);
            }
        }
        let Some(start) = start.or(default) else {
            return Ok(Completion::Normal(None));
        };
        let mut value = None;
        for case in &cases[start..] {
            let completion = self.statement_list(&case.consequent)?.update_empty(&value);
            match completion {
                Completion::Normal(next) => value = next,
                Completion::Break {
                    value,
                    target: None,
                } => {
                    return Ok(Completion::Normal(value));
                }
                abrupt => return Ok(abrupt),
            }
        }
        Ok(Completion::Normal(value))
    }

    fn try_statement(
        &self,
        block: &Stmt,
        handler: Option<&CatchClause>,
        finalizer: Option<&Stmt>,
    ) -> RuntimeResult<Completion> {
        let mut completion = self.statement(block, &[])?;
        if let (Completion::Throw(thrown), Some(handler)) = (&completion, handler) {
            completion = self.catch(handler, thrown.clone())?;
        }
        if let Some(finalizer) = finalizer {
            let final_completion = self.statement(finalizer, &[])?;
            if !final_completion.is_normal() {
                return Ok(final_completion);
            }
        }
        Ok(completion)
    }

    fn catch(&self, handler: &CatchClause, thrown: Value) -> RuntimeResult<Completion> {
        let environment = Environment::declarative(Some(self.expression.environment.clone()));
        if let Some(parameter) = &handler.param {
            let name = binding_name(parameter)?;
            environment.create_mutable_binding(name.clone(), false)?;
            environment.initialize_binding(&name, thrown)?;
        }
        self.nested(environment).statement(&handler.body, &[])
    }

    fn instantiate_lexical(
        &self,
        statements: &[Stmt],
        environment: &Environment,
    ) -> RuntimeResult<()> {
        for statement in statements {
            if let StmtKind::Variable { kind, declarations } = &statement.kind {
                if *kind != VariableKind::Var {
                    for declaration in declarations {
                        let name = binding_name(&declaration.binding)?;
                        if *kind == VariableKind::Const {
                            environment.create_immutable_binding(name, true)?;
                        } else {
                            environment.create_mutable_binding(name, false)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn instantiate_functions(&self, statements: &[Stmt]) -> RuntimeResult<()> {
        for statement in statements {
            let StmtKind::FunctionDeclaration(function) = &statement.kind else {
                continue;
            };
            let Some(Expr {
                kind: ExprKind::Identifier(name),
                ..
            }) = &function.name
            else {
                return Err(RuntimeError::syntax_error(
                    "function declaration requires a name",
                ));
            };
            let name = JsString::from(name.as_str());
            let function = create_function(function, FunctionKind::Normal, &self.expression, None)?;
            if self.expression.environment.has_binding(&name)? {
                let value = Value::Object(function);
                if self
                    .expression
                    .environment
                    .set_mutable_binding(&name, value.clone(), false)
                    .is_err()
                {
                    self.expression
                        .environment
                        .initialize_binding(&name, value)?;
                }
            } else {
                self.expression
                    .environment
                    .create_mutable_binding(name.clone(), false)?;
                self.expression
                    .environment
                    .initialize_binding(&name, Value::Object(function))?;
            }
        }
        Ok(())
    }

    fn instantiate_var_statements(&self, statements: &[Stmt]) -> RuntimeResult<()> {
        let mut names = Vec::new();
        collect_var_names(statements, &mut names)?;
        for name in names {
            if !self.variable_environment.has_binding(&name)? {
                self.variable_environment
                    .create_mutable_binding(name.clone(), true)?;
                self.variable_environment
                    .initialize_binding(&name, Value::Undefined)?;
            }
        }
        Ok(())
    }

    fn evaluate_declarations(
        &self,
        kind: VariableKind,
        declarations: &[VariableDeclarator],
    ) -> RuntimeResult<Completion> {
        for declaration in declarations {
            let name = binding_name(&declaration.binding)?;
            match kind {
                VariableKind::Var => {
                    if let Some(init) = &declaration.init {
                        self.variable_environment.set_mutable_binding(
                            &name,
                            self.expression.evaluate(init)?,
                            false,
                        )?;
                    }
                }
                VariableKind::Let | VariableKind::Const => {
                    let value = declaration
                        .init
                        .as_ref()
                        .map_or(Ok(Value::Undefined), |init| self.expression.evaluate(init))?;
                    self.expression
                        .environment
                        .initialize_binding(&name, value)?;
                }
            }
        }
        Ok(Completion::Normal(None))
    }

    fn evaluate_for_init(&self, init: &ForInit) -> RuntimeResult<()> {
        match init {
            ForInit::Expression(expression) => {
                self.expression.evaluate(expression)?;
            }
            ForInit::Variable { kind, declarations } => {
                if *kind != VariableKind::Var {
                    self.instantiate_declarations(*kind, declarations)?;
                }
                self.evaluate_declarations(*kind, declarations)?;
            }
        }
        Ok(())
    }

    fn instantiate_declarations(
        &self,
        kind: VariableKind,
        declarations: &[VariableDeclarator],
    ) -> RuntimeResult<()> {
        for declaration in declarations {
            let name = binding_name(&declaration.binding)?;
            if kind == VariableKind::Const {
                self.expression
                    .environment
                    .create_immutable_binding(name, true)?;
            } else {
                self.expression
                    .environment
                    .create_mutable_binding(name, false)?;
            }
        }
        Ok(())
    }

    fn assign_for_init(&self, init: &ForInit, value: Value) -> RuntimeResult<()> {
        match init {
            ForInit::Expression(expression) => self
                .expression
                .evaluate_reference(expression)?
                .put_value(value),
            ForInit::Variable { kind, declarations } => {
                let Some(declaration) = declarations.first() else {
                    return Err(RuntimeError::syntax_error("missing for-each binding"));
                };
                let name = binding_name(&declaration.binding)?;
                if *kind == VariableKind::Var {
                    return self
                        .variable_environment
                        .set_mutable_binding(&name, value, false);
                }
                if !self.expression.environment.has_binding(&name)? {
                    if *kind == VariableKind::Const {
                        self.expression
                            .environment
                            .create_immutable_binding(name.clone(), true)?;
                    } else {
                        self.expression
                            .environment
                            .create_mutable_binding(name.clone(), false)?;
                    }
                    self.expression.environment.initialize_binding(&name, value)
                } else {
                    self.expression
                        .environment
                        .set_mutable_binding(&name, value, true)
                }
            }
        }
    }
}

fn binding_name(expression: &Expr) -> RuntimeResult<JsString> {
    if let ExprKind::Identifier(name) = &expression.kind {
        Ok(JsString::from(name.as_str()))
    } else {
        Err(RuntimeError::internal_error(
            "destructuring binding execution is not implemented",
        ))
    }
}

fn collect_var_names(statements: &[Stmt], names: &mut Vec<JsString>) -> RuntimeResult<()> {
    for statement in statements {
        match &statement.kind {
            StmtKind::Variable {
                kind: VariableKind::Var,
                declarations,
            } => {
                for declaration in declarations {
                    let name = binding_name(&declaration.binding)?;
                    if !names.contains(&name) {
                        names.push(name);
                    }
                }
            }
            StmtKind::Block(body) => collect_var_names(body, names)?,
            StmtKind::If {
                consequent,
                alternate,
                ..
            } => {
                collect_var_names(std::slice::from_ref(consequent), names)?;
                if let Some(alternate) = alternate {
                    collect_var_names(std::slice::from_ref(alternate), names)?;
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::Labeled { body, .. }
            | StmtKind::With { body, .. } => {
                collect_var_names(std::slice::from_ref(body), names)?;
            }
            StmtKind::For { init, body, .. } => {
                collect_for_init_var(init.as_ref(), names)?;
                collect_var_names(std::slice::from_ref(body), names)?;
            }
            StmtKind::ForEach { left, body, .. } => {
                collect_for_init_var(Some(left), names)?;
                collect_var_names(std::slice::from_ref(body), names)?;
            }
            StmtKind::Try {
                block,
                handler,
                finalizer,
            } => {
                collect_var_names(std::slice::from_ref(block), names)?;
                if let Some(handler) = handler {
                    collect_var_names(std::slice::from_ref(&handler.body), names)?;
                }
                if let Some(finalizer) = finalizer {
                    collect_var_names(std::slice::from_ref(finalizer), names)?;
                }
            }
            StmtKind::Switch { cases, .. } => {
                for case in cases {
                    collect_var_names(&case.consequent, names)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn collect_for_init_var(init: Option<&ForInit>, names: &mut Vec<JsString>) -> RuntimeResult<()> {
    let Some(ForInit::Variable {
        kind: VariableKind::Var,
        declarations,
    }) = init
    else {
        return Ok(());
    };
    for declaration in declarations {
        let name = binding_name(&declaration.binding)?;
        if !names.contains(&name) {
            names.push(name);
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
        _ => Err(RuntimeError::type_error("value is not iterable")),
    }
}

fn enumerable_keys(value: &Value) -> RuntimeResult<Vec<Value>> {
    let Value::Object(object) = value else {
        if matches!(value, Value::Undefined | Value::Null) {
            return Err(RuntimeError::type_error(
                "cannot enumerate null or undefined",
            ));
        }
        return Ok(Vec::new());
    };
    Ok(object
        .own_property_keys()
        .into_iter()
        .filter(|key| {
            object
                .get_own_property(key)
                .is_some_and(|descriptor| descriptor.enumerable == Some(true))
        })
        .filter_map(|key| match key {
            PropertyKey::String(name) => Some(Value::String(name)),
            PropertyKey::Symbol(_) => None,
        })
        .collect())
}

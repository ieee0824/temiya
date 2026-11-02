//! Statement evaluation and ECMAScript Completion Records.

use crate::ast::{
    CatchClause, Expr, ExprKind, ForInit, Function, Program, Stmt, StmtKind, SwitchCase,
    VariableDeclarator, VariableKind,
};

use super::{
    Environment, EnvironmentKind, ExpressionEvaluator, FunctionKind, JsString, Object,
    ObjectEnvironmentOptions, PropertyKey, Realm, RuntimeError, RuntimeResult, Value,
    function::create_function, interpreter::strict_equal,
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
    pub(super) fn with_this_environment(mut self, environment: Option<Environment>) -> Self {
        self.expression.this_environment = environment;
        self
    }
    pub(super) fn with_function_cache(
        mut self,
        cache: Option<super::function::FunctionCache>,
    ) -> Self {
        self.expression.function_cache = cache;
        self
    }

    pub(crate) fn with_auto_functions(self) -> Self {
        self.with_function_cache(Some(Default::default()))
    }
    pub(super) fn with_new_target(mut self, new_target: Option<Object>) -> Self {
        self.expression.new_target = new_target;
        self
    }

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
        if evaluator.is_global_script() {
            evaluator.instantiate_global_script(&program.body)?;
        } else {
            evaluator.instantiate_var_statements(&program.body)?;
            evaluator.instantiate_lexical(&program.body, &evaluator.expression.environment)?;
            evaluator.instantiate_functions(&program.body)?;
        }
        evaluator.statement_list(&program.body)
    }

    /// Evaluates one statement.
    pub fn evaluate_statement(&self, statement: &Stmt) -> RuntimeResult<Completion> {
        if self.is_global_script() {
            self.instantiate_global_script(std::slice::from_ref(statement))?;
        } else {
            self.instantiate_var_statements(std::slice::from_ref(statement))?;
        }
        self.statement(statement, &[])
    }

    pub(super) fn evaluate_preinstantiated_try(
        &self,
        statement: &Stmt,
    ) -> RuntimeResult<Completion> {
        debug_assert!(matches!(statement.kind, StmtKind::Try { .. }));
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
                    let environment = Environment::declarative(self.expression.environment.clone());
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
                let environment = Environment::declarative(self.expression.environment.clone());
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
                    primitive => super::function::box_primitive(&self.expression.realm, primitive)?,
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
            StmtKind::ClassDeclaration(class) => {
                let name = class.name.as_ref().ok_or_else(|| {
                    RuntimeError::syntax_error("class declaration requires a name")
                })?;
                let name = binding_name(name)?;
                let constructor = super::class::evaluate(class, &self.expression, None)?;
                self.expression
                    .environment
                    .initialize_binding(&name, Value::Object(constructor))?;
                Ok(Completion::Normal(None))
            }
            StmtKind::ImportDeclaration(_) | StmtKind::ExportDeclaration(_) => {
                Err(RuntimeError::internal_error(
                    "function, class, and module declaration execution belongs to later milestones",
                ))
            }
        }
    }

    fn block(&self, statements: &[Stmt]) -> RuntimeResult<Completion> {
        // An empty declarative record cannot change name/this/super lookup.
        // Preserve actual block bindings and conservatively retain class/label
        // contexts; ordinary var declarations target variable_environment.
        let needs_environment = statements.iter().any(|statement| {
            matches!(
                &statement.kind,
                StmtKind::Variable {
                    kind: VariableKind::Let | VariableKind::Const,
                    ..
                } | StmtKind::FunctionDeclaration(_)
                    | StmtKind::ClassDeclaration(_)
                    | StmtKind::Labeled { .. }
            )
        });
        if !needs_environment {
            return self.statement_list(statements);
        }
        let environment = Environment::declarative(self.expression.environment.clone());
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
                    let environment = Environment::declarative(self.expression.environment.clone());
                    let nested = self.nested(environment);
                    nested.instantiate_declarations(*kind, declarations)?;
                    super::binding::initialize(
                        &nested.expression,
                        &declaration.binding,
                        value,
                        true,
                    )?;
                    nested.statement(body, &[])?
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
        let mut completion = self.statement(block, &[]);
        if let Some(handler) = handler {
            let thrown = match &completion {
                Ok(Completion::Throw(value)) => Some(value.clone()),
                Err(error) => Some(self.expression.realm.exception_value(error)?),
                _ => None,
            };
            if let Some(thrown) = thrown {
                completion = self.catch(handler, thrown);
            }
        }
        // Invariant/host panic failures are not JavaScript abrupt completions.
        // Neither catch nor finally may turn them into successful JS execution.
        if let Err(error) = &completion {
            if error.kind() == super::RuntimeErrorKind::InternalError {
                return completion;
            }
        }
        if let Some(finalizer) = finalizer {
            let final_completion = self.statement(finalizer, &[])?;
            if !final_completion.is_normal() {
                return Ok(final_completion);
            }
        }
        completion
    }

    fn catch(&self, handler: &CatchClause, thrown: Value) -> RuntimeResult<Completion> {
        let Some(parameter) = &handler.param else {
            return self.statement(&handler.body, &[]);
        };
        let environment = if matches!(parameter.kind, ExprKind::Identifier(_)) {
            Environment::declarative_initialized(
                self.expression.environment.clone(),
                binding_name(parameter)?,
                thrown,
            )?
        } else {
            let environment = Environment::declarative(self.expression.environment.clone());
            super::binding::for_each_name(parameter, |name| {
                environment.create_mutable_binding(name, false)
            })?;
            let nested = self.nested(environment.clone());
            super::binding::initialize(&nested.expression, parameter, thrown, true)?;
            environment
        };
        self.nested(environment).statement(&handler.body, &[])
    }

    fn is_global_script(&self) -> bool {
        self.variable_environment.kind() == EnvironmentKind::Global
            && self.variable_environment == self.expression.environment
    }

    // GlobalDeclarationInstantiation for the currently executable declaration
    // subset. Block functions and non-global code keep their existing rules.
    fn instantiate_global_script(&self, statements: &[Stmt]) -> RuntimeResult<()> {
        let environment = &self.variable_environment;
        let mut lexical: Vec<(JsString, bool)> = vec![];
        let mut vars = vec![];
        collect_var_names(statements, &mut vars)?;
        for statement in statements {
            if let StmtKind::ClassDeclaration(class) = &statement.kind {
                let name = binding_name(class.name.as_ref().ok_or_else(|| {
                    RuntimeError::syntax_error("class declaration requires a name")
                })?)?;
                if lexical.iter().any(|(other, _)| *other == name) {
                    return Err(RuntimeError::syntax_error("duplicate lexical declaration"));
                }
                lexical.push((name, false));
            }
            if let StmtKind::Variable { kind, declarations } = &statement.kind {
                if *kind != VariableKind::Var {
                    for declaration in declarations {
                        super::binding::for_each_name(&declaration.binding, |name| {
                            if lexical.iter().any(|(other, _)| *other == name) {
                                return Err(RuntimeError::syntax_error(
                                    "duplicate lexical declaration",
                                ));
                            }
                            lexical.push((name, *kind == VariableKind::Const));

                            Ok(())
                        })?;
                    }
                }
            }
        }
        // The last direct function declaration wins. Reverse again before
        // instantiation to preserve the surviving declarations' source order.
        let mut functions: Vec<(&Function, JsString)> = vec![];
        for statement in statements.iter().rev() {
            if let StmtKind::FunctionDeclaration(function) = &statement.kind {
                let name = function.name.as_ref().ok_or_else(|| {
                    RuntimeError::syntax_error("function declaration requires a name")
                })?;
                let name = binding_name(name)?;
                if !functions.iter().any(|(_, previous)| *previous == name) {
                    functions.push((function, name));
                }
            }
        }
        functions.reverse();
        for (name, _) in &lexical {
            if vars.contains(name)
                || functions.iter().any(|(_, other)| other == name)
                || !environment.can_declare_global_lexical(name)?
            {
                return Err(RuntimeError::syntax_error(
                    "global lexical declaration conflicts",
                ));
            }
        }
        for name in vars.iter().chain(functions.iter().map(|(_, name)| name)) {
            if environment.has_lexical_declaration(name)? {
                return Err(RuntimeError::syntax_error(
                    "global var/function conflicts with lexical declaration",
                ));
            }
        }
        for (_, name) in functions.iter().rev() {
            if !environment.can_declare_global_function(name)? {
                return Err(RuntimeError::type_error(
                    "global function declaration is not permitted",
                ));
            }
        }
        vars.retain(|name| !functions.iter().any(|(_, other)| other == name));
        for name in &vars {
            if !environment.can_declare_global_var(name)? {
                return Err(RuntimeError::type_error(
                    "global object rejects var declaration",
                ));
            }
        }
        // No binding or function object is created before all ordinary-global
        // declaration checks have completed successfully.
        for (name, immutable) in lexical {
            environment.create_global_lexical_binding(name, !immutable, immutable)?;
        }
        for (function, name) in functions {
            let function = create_function(function, FunctionKind::Normal, &self.expression, None)?;
            environment.create_global_function_binding(name, Value::Object(function), false)?;
        }
        for name in vars {
            environment.create_global_var_binding(name, false)?;
        }
        Ok(())
    }

    fn instantiate_lexical(
        &self,
        statements: &[Stmt],
        environment: &Environment,
    ) -> RuntimeResult<()> {
        for statement in statements {
            if let StmtKind::ClassDeclaration(class) = &statement.kind {
                let name = binding_name(class.name.as_ref().ok_or_else(|| {
                    RuntimeError::syntax_error("class declaration requires a name")
                })?)?;
                environment.create_mutable_binding(name, false)?;
            }
            if let StmtKind::Variable { kind, declarations } = &statement.kind {
                if *kind != VariableKind::Var {
                    for declaration in declarations {
                        super::binding::for_each_name(&declaration.binding, |name| {
                            if *kind == VariableKind::Const {
                                environment.create_immutable_binding(name, true)?;
                            } else {
                                environment.create_mutable_binding(name, false)?;
                            }

                            Ok(())
                        })?;
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
            if !matches!(declaration.binding.kind, ExprKind::Identifier(_)) {
                let value = declaration
                    .init
                    .as_ref()
                    .map_or(Ok(Value::Undefined), |init| self.expression.evaluate(init))?;
                super::binding::initialize(
                    &self.expression,
                    &declaration.binding,
                    value,
                    kind != VariableKind::Var,
                )?;
                continue;
            }
            let name = binding_name(&declaration.binding)?;
            match kind {
                VariableKind::Var => {
                    if let Some(init) = &declaration.init {
                        // Resolve before the RHS. An own Function binding has no
                        // observable HasBinding callback or intervening scope.
                        if self.expression.environment == self.variable_environment
                            && self.variable_environment.kind() == EnvironmentKind::Function
                            && self.variable_environment.has_binding(&name)?
                        {
                            let value = self.expression.evaluate_named(init, &name)?;
                            self.variable_environment.set_mutable_binding(
                                &name,
                                value,
                                self.expression.strict,
                            )?;
                        } else {
                            let reference =
                                self.expression.evaluate_reference(&declaration.binding)?;
                            let value = self.expression.evaluate_named(init, &name)?;
                            reference.put_value(value)?;
                        }
                    }
                }
                VariableKind::Let | VariableKind::Const => {
                    let value = declaration
                        .init
                        .as_ref()
                        .map_or(Ok(Value::Undefined), |init| {
                            // The module's synthetic default binding is not the
                            // function/class name visible to JavaScript.
                            if name.code_units() == [42, 100, 101, 102, 97, 117, 108, 116, 42]
                                && self.expression.environment.kind() == EnvironmentKind::Module
                            {
                                self.expression
                                    .evaluate_named(init, &JsString::from("default"))
                            } else {
                                self.expression.evaluate_named(init, &name)
                            }
                        })?;
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
            super::binding::for_each_name(&declaration.binding, |name| {
                if kind == VariableKind::Const {
                    self.expression
                        .environment
                        .create_immutable_binding(name, true)?;
                } else {
                    self.expression
                        .environment
                        .create_mutable_binding(name, false)?;
                }

                Ok(())
            })?;
        }
        Ok(())
    }

    fn assign_for_init(&self, init: &ForInit, value: Value) -> RuntimeResult<()> {
        match init {
            ForInit::Expression(expression) => {
                if matches!(
                    expression.kind,
                    ExprKind::Array { .. } | ExprKind::Object(_)
                ) {
                    super::binding::initialize(&self.expression, expression, value, false)
                } else {
                    self.expression
                        .evaluate_reference(expression)?
                        .put_value(value)
                }
            }
            ForInit::Variable { kind, declarations } => {
                let Some(declaration) = declarations.first() else {
                    return Err(RuntimeError::syntax_error("missing for-each binding"));
                };
                if !matches!(declaration.binding.kind, ExprKind::Identifier(_)) {
                    if *kind != VariableKind::Var {
                        self.instantiate_declarations(*kind, declarations)?;
                    }
                    return super::binding::initialize(
                        &self.expression,
                        &declaration.binding,
                        value,
                        *kind != VariableKind::Var,
                    );
                }
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
                    super::binding::for_each_name(&declaration.binding, |name| {
                        if !names.contains(&name) {
                            names.push(name);
                        }

                        Ok(())
                    })?;
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
        super::binding::for_each_name(&declaration.binding, |name| {
            if !names.contains(&name) {
                names.push(name);
            }

            Ok(())
        })?;
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

pub(super) struct StoredStatementEvaluator {
    expression: super::interpreter::StoredExpressionEvaluator,
    variable_environment: super::environment::StoredEnvironment,
}
impl StoredStatementEvaluator {
    pub(super) fn store(owner: &Object, evaluator: StatementEvaluator) -> RuntimeResult<Self> {
        Ok(Self {
            expression: super::interpreter::StoredExpressionEvaluator::store(
                owner,
                evaluator.expression,
            )?,
            variable_environment: super::environment::StoredEnvironment::store(
                owner,
                &evaluator.variable_environment,
            )?,
        })
    }
    pub(super) fn load(self, owner: &Object) -> RuntimeResult<StatementEvaluator> {
        Ok(StatementEvaluator {
            expression: self.expression.load(owner)?,
            variable_environment: self.variable_environment.load(owner)?,
        })
    }
    pub(super) fn trace(&self, tracer: &mut crate::gc::Tracer<'_>) {
        self.expression.trace(tracer);
        self.variable_environment.trace(tracer);
    }
}

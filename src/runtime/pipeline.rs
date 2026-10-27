//! Script parsing/evaluation and cyclic Source Text Module linking.

use std::{
    cell::RefCell,
    collections::HashSet,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{
    ast::{
        ExportKind, Expr, ExprKind, ImportName, ModuleRequest, Program, Stmt, StmtKind,
        VariableDeclarator, VariableKind,
    },
    parser::{parse_module, parse_script},
    semantics::{DeclarationScope, bound_names, lexically_declared_names, var_declared_names},
};

use super::{
    Completion, Environment, JsString, Object, Promise, PropertyDescriptor, PropertyKey, Realm,
    RuntimeError, RuntimeResult, ScriptRecord, StatementEvaluator, Value,
};

impl ScriptRecord {
    /// Parses Script source and associates it with a Realm.
    pub fn parse(realm: Realm, source: &str) -> RuntimeResult<Self> {
        parse_script(source)
            .map(|program| Self::new(realm, program))
            .map_err(|error| RuntimeError::syntax_error(error.to_string()))
    }

    /// Evaluates this Script in its Realm's global environment.
    pub fn evaluate(&self) -> RuntimeResult<Completion> {
        let realm = self.realm();
        StatementEvaluator::new(realm)
            .with_function_code_cache(Some(self.function_code_cache()))
            .evaluate_program(self.program())
    }
}

/// Lifecycle of a Source Text Module Record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModuleStatus {
    /// Parsed but not linked.
    Unlinked,
    /// Currently participating in cyclic linking.
    Linking,
    /// Instantiated and ready to evaluate.
    Linked,
    /// Currently participating in cyclic evaluation.
    Evaluating,
    /// Waiting at a top-level await boundary.
    EvaluatingAsync,
    /// Evaluation completed successfully.
    Evaluated,
    /// Linking or evaluation failed; the failure is cached.
    Errored,
}

#[derive(Clone)]
struct Dependency {
    request: ModuleRequest,
    module: SourceTextModule,
}

struct ModuleData {
    id: u64,
    specifier: JsString,
    realm: Realm,
    program: Program,
    environment: Environment,
    dependencies: RefCell<Vec<Dependency>>,
    status: RefCell<ModuleStatus>,
    error: RefCell<Option<RuntimeError>>,
    evaluation: RefCell<Option<Promise>>,
}

/// A parsed Source Text Module Record with cached link/evaluation state.
#[derive(Clone)]
pub struct SourceTextModule(Rc<ModuleData>);

impl std::fmt::Debug for SourceTextModule {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SourceTextModule")
            .field("specifier", &self.specifier())
            .field("status", &self.status())
            .finish_non_exhaustive()
    }
}

impl SourceTextModule {
    fn new(specifier: JsString, realm: Realm, program: Program) -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        Self(Rc::new(ModuleData {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            specifier,
            environment: Environment::module(Some(realm.global_environment())),
            realm,
            program,
            dependencies: RefCell::new(Vec::new()),
            status: RefCell::new(ModuleStatus::Unlinked),
            error: RefCell::new(None),
            evaluation: RefCell::new(None),
        }))
    }

    /// Returns the host-facing module specifier.
    #[must_use]
    pub fn specifier(&self) -> JsString {
        self.0.specifier.clone()
    }

    /// Returns the current link/evaluation state.
    #[must_use]
    pub fn status(&self) -> ModuleStatus {
        *self.0.status.borrow()
    }

    /// Returns the Module Environment Record.
    #[must_use]
    pub fn environment(&self) -> Environment {
        self.0.environment.clone()
    }

    /// Links this module and every transitively requested module.
    pub fn link(&self) -> RuntimeResult<()> {
        match self.status() {
            ModuleStatus::Linked
            | ModuleStatus::Evaluating
            | ModuleStatus::EvaluatingAsync
            | ModuleStatus::Evaluated => return Ok(()),
            ModuleStatus::Linking => return Ok(()),
            ModuleStatus::Errored => return Err(self.cached_error()),
            ModuleStatus::Unlinked => {}
        }
        *self.0.status.borrow_mut() = ModuleStatus::Linking;
        let result: RuntimeResult<()> = (|| {
            self.instantiate_local_bindings()?;
            for dependency in self.0.dependencies.borrow().iter() {
                dependency.module.link()?;
            }
            self.instantiate_namespace_exports()?;
            self.instantiate_imports()?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                *self.0.status.borrow_mut() = ModuleStatus::Linked;
                Ok(())
            }
            Err(error) => {
                self.cache_error(error.clone());
                Err(error)
            }
        }
    }

    /// Evaluates dependencies and this module once, returning a cached Promise.
    pub fn evaluate(&self) -> RuntimeResult<Promise> {
        match self.status() {
            ModuleStatus::Evaluated | ModuleStatus::EvaluatingAsync => {
                return self.0.evaluation.borrow().clone().ok_or_else(|| {
                    RuntimeError::internal_error("evaluated module has no evaluation Promise")
                });
            }
            ModuleStatus::Evaluating => {
                return Promise::fulfilled_on(self.0.realm.job_queue(), Value::Undefined);
            }
            ModuleStatus::Errored => return Err(self.cached_error()),
            ModuleStatus::Unlinked | ModuleStatus::Linking => self.link()?,
            ModuleStatus::Linked => {}
        }
        *self.0.status.borrow_mut() = ModuleStatus::Evaluating;
        let result = (|| {
            for dependency in self.0.dependencies.borrow().iter() {
                dependency.module.evaluate()?;
            }
            let body = self.executable_body()?;
            let evaluator = StatementEvaluator::with_environments(
                self.0.realm.clone(),
                self.0.environment.clone(),
                self.0.environment.clone(),
                Value::Undefined,
                true,
            );
            match evaluator.evaluate_instantiated(&body)? {
                Completion::Normal(value) => Ok(value.unwrap_or(Value::Undefined)),
                Completion::Throw(value) => Err(RuntimeError::thrown(value)),
                _ => Err(RuntimeError::internal_error(
                    "invalid completion escaped module evaluation",
                )),
            }
        })();
        match result {
            Ok(value) => {
                let asynchronous = program_has_top_level_await(&self.0.program);
                if asynchronous {
                    *self.0.status.borrow_mut() = ModuleStatus::EvaluatingAsync;
                }
                let promise = Promise::fulfilled_on(self.0.realm.job_queue(), value)?;
                *self.0.evaluation.borrow_mut() = Some(promise.clone());
                *self.0.status.borrow_mut() = ModuleStatus::Evaluated;
                Ok(promise)
            }
            Err(error) => {
                self.cache_error(error.clone());
                Err(error)
            }
        }
    }

    /// Reads an exported live binding after resolving re-exports.
    pub fn get_export(&self, name: &JsString) -> RuntimeResult<Value> {
        let (module, binding) = self.resolve_export(name, &mut Vec::new())?;
        module.0.environment.get_binding_value(&binding, true)
    }

    fn instantiate_local_bindings(&self) -> RuntimeResult<()> {
        let imported = imported_local_names(&self.0.program);
        for name in lexically_declared_names(&self.0.program.body, DeclarationScope::Module) {
            let name = JsString::from(name.name);
            if imported.contains(&name) || self.0.environment.has_binding(&name)? {
                continue;
            }
            self.0.environment.create_mutable_binding(name, false)?;
        }
        for name in var_declared_names(&self.0.program.body, DeclarationScope::Module) {
            let name = JsString::from(name.name);
            if !self.0.environment.has_binding(&name)? {
                self.0
                    .environment
                    .create_mutable_binding(name.clone(), false)?;
                self.0
                    .environment
                    .initialize_binding(&name, Value::Undefined)?;
            }
        }
        Ok(())
    }

    fn instantiate_imports(&self) -> RuntimeResult<()> {
        for statement in &self.0.program.body {
            let StmtKind::ImportDeclaration(import) = &statement.kind else {
                continue;
            };
            let dependency = self.dependency(&import.request)?;
            for specifier in &import.specifiers {
                let local = identifier_name(&specifier.local)?;
                match &specifier.imported {
                    ImportName::Name(imported) => {
                        let imported = JsString::from_code_units(imported.clone());
                        let (target, binding) =
                            dependency.resolve_export(&imported, &mut Vec::new())?;
                        self.0.environment.create_import_binding(
                            local,
                            target.0.environment.clone(),
                            binding,
                        )?;
                    }
                    ImportName::Namespace => {
                        self.0
                            .environment
                            .create_immutable_binding(local.clone(), true)?;
                        self.0.environment.initialize_binding(
                            &local,
                            Value::Object(dependency.namespace_object()?),
                        )?;
                    }
                }
            }
        }
        Ok(())
    }

    fn instantiate_namespace_exports(&self) -> RuntimeResult<()> {
        for statement in &self.0.program.body {
            let StmtKind::ExportDeclaration(export) = &statement.kind else {
                continue;
            };
            let ExportKind::All {
                exported: Some(_),
                request,
            } = &export.kind
            else {
                continue;
            };
            let name = namespace_binding_name(request);
            if !self.0.environment.has_binding(&name)? {
                self.0
                    .environment
                    .create_immutable_binding(name.clone(), true)?;
                let namespace = self.dependency(request)?.namespace_object()?;
                self.0
                    .environment
                    .initialize_binding(&name, Value::Object(namespace))?;
            }
        }
        Ok(())
    }

    fn resolve_export(
        &self,
        name: &JsString,
        resolve_set: &mut Vec<(u64, JsString)>,
    ) -> RuntimeResult<(Self, JsString)> {
        if resolve_set
            .iter()
            .any(|(id, candidate)| *id == self.0.id && candidate == name)
        {
            return Err(RuntimeError::syntax_error("circular export resolution"));
        }
        resolve_set.push((self.0.id, name.clone()));
        for statement in &self.0.program.body {
            let StmtKind::ExportDeclaration(export) = &statement.kind else {
                continue;
            };
            match &export.kind {
                ExportKind::Named {
                    specifiers,
                    request,
                } => {
                    for specifier in specifiers {
                        if JsString::from_code_units(specifier.exported.clone()) == *name {
                            let local = JsString::from_code_units(specifier.local.clone());
                            return if let Some(request) = request {
                                self.dependency(request)?
                                    .resolve_export(&local, resolve_set)
                            } else {
                                Ok((self.clone(), local))
                            };
                        }
                    }
                }
                ExportKind::Declaration(declaration) => {
                    for local in statement_bound_names(declaration) {
                        if local == *name {
                            return Ok((self.clone(), local));
                        }
                    }
                }
                ExportKind::DefaultDeclaration(declaration) => {
                    if name == &JsString::from("default") {
                        let local = statement_bound_names(declaration)
                            .into_iter()
                            .next()
                            .unwrap_or_else(|| JsString::from("*default*"));
                        return Ok((self.clone(), local));
                    }
                }
                ExportKind::DefaultExpression(_) if name == &JsString::from("default") => {
                    return Ok((self.clone(), JsString::from("*default*")));
                }
                ExportKind::All {
                    exported: Some(exported),
                    request,
                } if JsString::from_code_units(exported.clone()) == *name => {
                    return Ok((self.clone(), namespace_binding_name(request)));
                }
                _ => {}
            }
        }
        let mut found: Option<(Self, JsString)> = None;
        if name != &JsString::from("default") {
            for statement in &self.0.program.body {
                let StmtKind::ExportDeclaration(export) = &statement.kind else {
                    continue;
                };
                if let ExportKind::All {
                    exported: None,
                    request,
                } = &export.kind
                {
                    if let Ok(candidate) =
                        self.dependency(request)?.resolve_export(name, resolve_set)
                    {
                        if found.as_ref().is_some_and(|existing| {
                            existing.0.0.id != candidate.0.0.id || existing.1 != candidate.1
                        }) {
                            return Err(RuntimeError::syntax_error("ambiguous star export"));
                        }
                        found = Some(candidate);
                    }
                }
            }
        }
        found.ok_or_else(|| {
            RuntimeError::syntax_error(format!("export {} was not found", name.to_string_lossy()))
        })
    }

    fn namespace_object(&self) -> RuntimeResult<Object> {
        let object = Object::new();
        for name in self.exported_names(&mut HashSet::new())? {
            let (target, binding) = self.resolve_export(&name, &mut Vec::new())?;
            let getter = Object::native_function(move |_, _| {
                target.0.environment.get_binding_value(&binding, true)
            });
            getter.initialize_prototype(Some(
                self.0
                    .realm
                    .intrinsic(super::IntrinsicId::FunctionPrototype),
            ));
            object.define_own_property(
                PropertyKey::String(name),
                PropertyDescriptor::accessor(Some(getter), None, true, false),
            )?;
        }
        object.prevent_extensions();
        Ok(object)
    }

    fn exported_names(&self, seen: &mut HashSet<u64>) -> RuntimeResult<Vec<JsString>> {
        if !seen.insert(self.0.id) {
            return Ok(Vec::new());
        }
        let mut names = Vec::new();
        for statement in &self.0.program.body {
            let StmtKind::ExportDeclaration(export) = &statement.kind else {
                continue;
            };
            match &export.kind {
                ExportKind::Named { specifiers, .. } => names.extend(
                    specifiers
                        .iter()
                        .map(|s| JsString::from_code_units(s.exported.clone())),
                ),
                ExportKind::Declaration(statement) => {
                    names.extend(statement_bound_names(statement))
                }
                ExportKind::DefaultDeclaration(_) | ExportKind::DefaultExpression(_) => {
                    names.push(JsString::from("default"))
                }
                ExportKind::All {
                    exported: Some(name),
                    ..
                } => names.push(JsString::from_code_units(name.clone())),
                ExportKind::All {
                    exported: None,
                    request,
                } => names.extend(
                    self.dependency(request)?
                        .exported_names(seen)?
                        .into_iter()
                        .filter(|name| name != &JsString::from("default")),
                ),
            }
        }
        names.sort();
        names.dedup();
        Ok(names)
    }

    fn executable_body(&self) -> RuntimeResult<Vec<Stmt>> {
        let mut body = Vec::new();
        for statement in &self.0.program.body {
            match &statement.kind {
                StmtKind::ImportDeclaration(_) => {}
                StmtKind::ExportDeclaration(export) => match &export.kind {
                    ExportKind::Declaration(statement) => body.push((**statement).clone()),
                    ExportKind::DefaultDeclaration(statement) => {
                        if statement_bound_names(statement).is_empty() {
                            let StmtKind::FunctionDeclaration(function) = &statement.kind else {
                                return Err(RuntimeError::internal_error(
                                    "anonymous default class evaluation is not implemented",
                                ));
                            };
                            body.push(default_variable(
                                statement.span,
                                Expr {
                                    span: function.span,
                                    kind: ExprKind::Function(function.clone()),
                                },
                            ));
                        } else {
                            body.push((**statement).clone());
                        }
                    }
                    ExportKind::DefaultExpression(expression) => {
                        body.push(default_variable(expression.span, expression.clone()))
                    }
                    ExportKind::Named { .. } | ExportKind::All { .. } => {}
                },
                _ => body.push(statement.clone()),
            }
        }
        Ok(body)
    }

    fn dependency(&self, request: &ModuleRequest) -> RuntimeResult<Self> {
        self.0
            .dependencies
            .borrow()
            .iter()
            .find(|dependency| same_request(&dependency.request, request))
            .map(|dependency| dependency.module.clone())
            .ok_or_else(|| RuntimeError::internal_error("resolved module dependency is missing"))
    }

    fn cached_error(&self) -> RuntimeError {
        self.0.error.borrow().clone().unwrap_or_else(|| {
            RuntimeError::internal_error("module is in errored state without an error")
        })
    }
    fn cache_error(&self, error: RuntimeError) {
        *self.0.error.borrow_mut() = Some(error);
        *self.0.status.borrow_mut() = ModuleStatus::Errored;
    }
}

type ResolveCallback = dyn Fn(&ModuleRequest) -> RuntimeResult<String>;

/// Host-controlled module graph loader with per-specifier caching.
#[derive(Clone)]
pub struct ModuleGraph {
    realm: Realm,
    resolve: Rc<ResolveCallback>,
    modules: Rc<RefCell<Vec<(ModuleKey, SourceTextModule)>>>,
}

impl std::fmt::Debug for ModuleGraph {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ModuleGraph")
            .field("module_count", &self.modules.borrow().len())
            .finish_non_exhaustive()
    }
}

impl ModuleGraph {
    /// Creates a graph around a host source resolver.
    pub fn new(
        realm: Realm,
        resolve: impl Fn(&ModuleRequest) -> RuntimeResult<String> + 'static,
    ) -> Self {
        Self {
            realm,
            resolve: Rc::new(resolve),
            modules: Rc::new(RefCell::new(Vec::new())),
        }
    }

    /// Resolves and parses a root module and all of its static dependencies.
    pub fn load(&self, specifier: impl Into<JsString>) -> RuntimeResult<SourceTextModule> {
        let specifier = specifier.into();
        let request = ModuleRequest {
            span: crate::lexer::Span { start: 0, end: 0 },
            specifier: specifier.code_units().to_vec(),
            attributes: Vec::new(),
        };
        self.load_request(&request)
    }

    fn load_request(&self, request: &ModuleRequest) -> RuntimeResult<SourceTextModule> {
        let specifier = JsString::from_code_units(request.specifier.clone());
        let key = ModuleKey::from_request(request);
        if let Some(module) = self
            .modules
            .borrow()
            .iter()
            .find(|(candidate, _)| candidate == &key)
            .map(|(_, module)| module.clone())
        {
            return Ok(module);
        }
        let source = (self.resolve)(request)?;
        let program =
            parse_module(&source).map_err(|error| RuntimeError::syntax_error(error.to_string()))?;
        let module = SourceTextModule::new(specifier.clone(), self.realm.clone(), program.clone());
        self.modules.borrow_mut().push((key, module.clone()));
        let mut dependencies = Vec::new();
        for request in program.module_requests() {
            dependencies.push(Dependency {
                module: self.load_request(&request)?,
                request,
            });
        }
        *module.0.dependencies.borrow_mut() = dependencies;
        Ok(module)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ModuleKey {
    specifier: Vec<u16>,
    attributes: Vec<(Vec<u16>, Vec<u16>)>,
}

impl ModuleKey {
    fn from_request(request: &ModuleRequest) -> Self {
        let mut attributes = request
            .attributes
            .iter()
            .map(|attribute| (attribute.key.clone(), attribute.value.clone()))
            .collect::<Vec<_>>();
        attributes.sort_by(|left, right| left.0.cmp(&right.0));
        Self {
            specifier: request.specifier.clone(),
            attributes,
        }
    }
}

fn identifier_name(expression: &Expr) -> RuntimeResult<JsString> {
    if let ExprKind::Identifier(name) = &expression.kind {
        Ok(JsString::from(name.as_str()))
    } else {
        Err(RuntimeError::syntax_error("expected identifier binding"))
    }
}
fn imported_local_names(program: &Program) -> Vec<JsString> {
    program
        .body
        .iter()
        .filter_map(|statement| {
            if let StmtKind::ImportDeclaration(import) = &statement.kind {
                Some(import.specifiers.iter())
            } else {
                None
            }
        })
        .flatten()
        .filter_map(|specifier| identifier_name(&specifier.local).ok())
        .collect()
}
fn statement_bound_names(statement: &Stmt) -> Vec<JsString> {
    match &statement.kind {
        StmtKind::Variable { declarations, .. } => declarations
            .iter()
            .flat_map(|declaration| bound_names(&declaration.binding))
            .map(|name| JsString::from(name.name))
            .collect(),
        StmtKind::FunctionDeclaration(function) => function
            .name
            .iter()
            .flat_map(bound_names)
            .map(|name| JsString::from(name.name))
            .collect(),
        StmtKind::ClassDeclaration(class) => class
            .name
            .iter()
            .flat_map(bound_names)
            .map(|name| JsString::from(name.name))
            .collect(),
        _ => Vec::new(),
    }
}
fn default_variable(span: crate::lexer::Span, init: Expr) -> Stmt {
    Stmt {
        span,
        kind: StmtKind::Variable {
            kind: VariableKind::Const,
            declarations: vec![VariableDeclarator {
                span,
                binding: Expr {
                    span,
                    kind: ExprKind::Identifier("*default*".to_owned()),
                },
                init: Some(init),
            }],
        },
    }
}
fn same_request(left: &ModuleRequest, right: &ModuleRequest) -> bool {
    left.specifier == right.specifier
        && left
            .attributes
            .iter()
            .map(|a| (&a.key, &a.value))
            .eq(right.attributes.iter().map(|a| (&a.key, &a.value)))
}
fn namespace_binding_name(request: &ModuleRequest) -> JsString {
    JsString::from(format!(
        "*namespace:{}",
        String::from_utf16_lossy(&request.specifier)
    ))
}
fn program_has_top_level_await(program: &Program) -> bool {
    fn expr(e: &Expr) -> bool {
        crate::semantics::contains(e, crate::semantics::SyntaxFeature::Await)
    }
    program.body.iter().any(|statement| match &statement.kind {
        StmtKind::Expression(e) | StmtKind::Throw(e) => expr(e),
        StmtKind::Return(Some(e)) => expr(e),
        StmtKind::Variable { declarations, .. } => declarations
            .iter()
            .any(|d| d.init.as_ref().is_some_and(expr)),
        StmtKind::ExportDeclaration(export) => match &export.kind {
            ExportKind::DefaultExpression(e) => expr(e),
            _ => false,
        },
        _ => false,
    })
}

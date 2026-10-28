//! Script parsing/evaluation and cyclic Source Text Module linking.

use std::{
    cell::RefCell,
    collections::HashSet,
    ops::Deref,
    rc::{Rc, Weak},
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
use super::{
    Runtime,
    environment::StoredEnvironment,
    execution::StoredRealm,
    stored::{StoredObject, StoredOwner, StoredRuntimeError},
};
use crate::gc::{Handle, Root, Trace, Tracer};

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
        StatementEvaluator::new(realm).evaluate_program(self.program())
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
    module: StoredModule,
}

pub(super) struct ModuleData {
    pub(super) facade: RefCell<Weak<ModuleRoot>>,
    id: u64,
    specifier: JsString,
    realm: StoredRealm,
    program: Program,
    environment: StoredEnvironment,
    dependencies: RefCell<Vec<Dependency>>,
    status: RefCell<ModuleStatus>,
    error: RefCell<Option<StoredRuntimeError>>,
    evaluation: RefCell<Option<StoredObject>>,
    namespace: RefCell<Option<StoredObject>>,
}

/// A parsed Source Text Module Record with cached link/evaluation state.
#[derive(Clone)]
pub struct SourceTextModule(pub(super) ModuleReference);

#[derive(Clone)]
pub(super) enum ModuleReference {
    Legacy(Rc<ModuleData>),
    Managed(Rc<ModuleRoot>),
}
impl Deref for ModuleReference {
    type Target = ModuleData;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Legacy(data) => data,
            Self::Managed(root) => &root.data,
        }
    }
}
pub(super) struct ModuleRoot {
    pub(super) root: Root<ManagedModule>,
    pub(super) data: Rc<ModuleData>,
    pub(super) runtime: Runtime,
}
pub(super) struct ManagedModule {
    pub(super) data: Rc<ModuleData>,
}
impl Trace for ManagedModule {
    fn trace(&self, tracer: &mut Tracer<'_>) {
        let data = &self.data;
        data.realm.trace(tracer);
        data.environment.trace(tracer);
        for dependency in data.dependencies.borrow().iter() {
            dependency.module.trace(tracer);
        }
        if let Some(error) = &*data.error.borrow() {
            error.trace(tracer);
        }
        for object in [&data.evaluation, &data.namespace] {
            if let Some(object) = &*object.borrow() {
                object.trace(tracer);
            }
        }
    }
}
impl StoredOwner for SourceTextModule {
    type Allocation = ManagedModule;
    fn runtime(&self) -> Option<&Runtime> {
        match &self.0 {
            ModuleReference::Legacy(_) => None,
            ModuleReference::Managed(root) => Some(&root.runtime),
        }
    }
    fn handle(&self) -> Option<Handle<ManagedModule>> {
        match &self.0 {
            ModuleReference::Legacy(_) => None,
            ModuleReference::Managed(root) => Some(root.root.handle()),
        }
    }
}
#[derive(Clone)]
pub(super) enum StoredModule {
    Legacy(SourceTextModule),
    Managed(Handle<ManagedModule>),
}
impl StoredModule {
    fn initial(runtime: Option<&Runtime>, module: &SourceTextModule) -> RuntimeResult<Self> {
        match (runtime, module.runtime()) {
            (None, None) => Ok(Self::Legacy(module.clone())),
            (Some(left), Some(right)) if left.same_heap(right) => {
                Ok(Self::Managed(module.handle().expect("managed module")))
            }
            _ => Err(RuntimeError::type_error(
                "Module allocation context mismatch",
            )),
        }
    }
    fn store(owner: &impl StoredOwner, module: &SourceTextModule) -> RuntimeResult<Self> {
        let stored = Self::initial(owner.runtime(), module)?;
        if let (Self::Managed(target), Some(runtime)) = (&stored, owner.runtime()) {
            runtime.record_object_edge(owner.handle().expect("managed owner"), *target)?;
        }
        Ok(stored)
    }
    fn load(&self, owner: &impl StoredOwner) -> RuntimeResult<SourceTextModule> {
        match (self, owner.runtime()) {
            (Self::Legacy(module), None) => Ok(module.clone()),
            (Self::Managed(handle), Some(runtime)) => runtime
                .root_module(*handle)
                .ok_or_else(|| RuntimeError::internal_error("stale module edge")),
            _ => Err(RuntimeError::internal_error(
                "Module allocation context mismatch",
            )),
        }
    }
    pub(super) fn trace(&self, tracer: &mut Tracer<'_>) {
        if let Self::Managed(handle) = self {
            tracer.mark(*handle);
        }
    }
}

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
    fn new(specifier: JsString, realm: Realm, program: Program) -> RuntimeResult<Self> {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let environment = Environment::module(Some(realm.global_environment()));
        let data = ModuleData {
            facade: RefCell::new(Weak::new()),
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            specifier,
            environment: StoredEnvironment::initial(realm.runtime(), &environment)?,
            realm: StoredRealm::initial(realm.runtime(), &realm)?,
            program,
            dependencies: RefCell::new(Vec::new()),
            status: RefCell::new(ModuleStatus::Unlinked),
            error: RefCell::new(None),
            evaluation: RefCell::new(None),
            namespace: RefCell::new(None),
        };
        Ok(match realm.runtime() {
            Some(runtime) => runtime.allocate_module(data),
            None => Self(ModuleReference::Legacy(Rc::new(data))),
        })
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
        self.0
            .environment
            .load(self)
            .expect("live module environment")
    }

    fn realm(&self) -> Realm {
        self.0.realm.load(self).expect("live module Realm")
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
                dependency.module.load(self)?.link()?;
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
                self.cache_error(error.clone())?;
                Err(error)
            }
        }
    }

    /// Evaluates dependencies and this module once, returning a cached Promise.
    pub fn evaluate(&self) -> RuntimeResult<Promise> {
        match self.status() {
            ModuleStatus::Evaluated | ModuleStatus::EvaluatingAsync => {
                return self
                    .0
                    .evaluation
                    .borrow()
                    .as_ref()
                    .map(|stored| {
                        Promise::from_object(stored.load(self)?).ok_or_else(|| {
                            RuntimeError::internal_error("module evaluation slot is not a Promise")
                        })
                    })
                    .transpose()?
                    .ok_or_else(|| {
                        RuntimeError::internal_error("evaluated module has no evaluation Promise")
                    });
            }
            ModuleStatus::Evaluating => {
                return Promise::settled_in(&self.realm(), Ok(Value::Undefined));
            }
            ModuleStatus::Errored => return Err(self.cached_error()),
            ModuleStatus::Unlinked | ModuleStatus::Linking => self.link()?,
            ModuleStatus::Linked => {}
        }
        *self.0.status.borrow_mut() = ModuleStatus::Evaluating;
        let result = (|| {
            for dependency in self.0.dependencies.borrow().iter() {
                dependency.module.load(self)?.evaluate()?;
            }
            let body = self.executable_body()?;
            let evaluator = StatementEvaluator::with_environments(
                self.realm(),
                self.environment(),
                self.environment(),
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
                let promise = Promise::settled_in(&self.realm(), Ok(value))?;
                *self.0.evaluation.borrow_mut() =
                    Some(StoredObject::store(self, &promise.object())?);
                *self.0.status.borrow_mut() = ModuleStatus::Evaluated;
                Ok(promise)
            }
            Err(error) => {
                self.cache_error(error.clone())?;
                Err(error)
            }
        }
    }

    /// Reads an exported live binding after resolving re-exports.
    pub fn get_export(&self, name: &JsString) -> RuntimeResult<Value> {
        let (module, binding) = self.resolve_export(name, &mut Vec::new())?;
        module.environment().get_binding_value(&binding, true)
    }

    fn instantiate_local_bindings(&self) -> RuntimeResult<()> {
        let imported = imported_local_names(&self.0.program);
        for name in lexically_declared_names(&self.0.program.body, DeclarationScope::Module) {
            let name = JsString::from(name.name);
            if imported.contains(&name) || self.environment().has_binding(&name)? {
                continue;
            }
            self.environment().create_mutable_binding(name, false)?;
        }
        for name in var_declared_names(&self.0.program.body, DeclarationScope::Module) {
            let name = JsString::from(name.name);
            if !self.environment().has_binding(&name)? {
                self.environment()
                    .create_mutable_binding(name.clone(), false)?;
                self.environment()
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
                        self.environment().create_import_binding(
                            local,
                            target.environment(),
                            binding,
                        )?;
                    }
                    ImportName::Namespace => {
                        self.environment()
                            .create_immutable_binding(local.clone(), true)?;
                        self.environment().initialize_binding(
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
            if !self.environment().has_binding(&name)? {
                self.environment()
                    .create_immutable_binding(name.clone(), true)?;
                let namespace = self.dependency(request)?.namespace_object()?;
                self.environment()
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
        if let Some(namespace) = &*self.0.namespace.borrow() {
            return namespace.load(self);
        }
        let realm = self.realm();
        let object = Object::new_for(&realm.global_object());
        object.0.state.borrow_mut().module_namespace =
            Some(Box::new(StoredModule::store(&object, self)?));
        for name in self.exported_names(&mut HashSet::new())? {
            let (target, binding) = self.resolve_export(&name, &mut Vec::new())?;
            let getter = Object::managed_function_with_metadata(
                &realm.intrinsic(super::IntrinsicId::FunctionPrototype),
                "get",
                0,
                super::object::ManagedCallable::ModuleBinding(ModuleBinding {
                    module: StoredModule::initial(self.runtime(), &target)?,
                    binding,
                }),
            );
            object.define_own_property(
                PropertyKey::String(name),
                PropertyDescriptor::accessor(Some(getter), None, true, false),
            )?;
        }
        object.prevent_extensions();
        *self.0.namespace.borrow_mut() = Some(StoredObject::store(self, &object)?);
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
            .map(|dependency| dependency.module.load(self))
            .transpose()?
            .ok_or_else(|| RuntimeError::internal_error("resolved module dependency is missing"))
    }

    fn cached_error(&self) -> RuntimeError {
        self.0
            .error
            .borrow()
            .as_ref()
            .map(|error| error.load(self).unwrap_or_else(|failure| failure))
            .unwrap_or_else(|| {
                RuntimeError::internal_error("module is in errored state without an error")
            })
    }
    fn cache_error(&self, error: RuntimeError) -> RuntimeResult<()> {
        *self.0.error.borrow_mut() = Some(StoredRuntimeError::store(self, error)?);
        *self.0.status.borrow_mut() = ModuleStatus::Errored;
        Ok(())
    }
}

pub(super) struct ModuleBinding {
    module: StoredModule,
    binding: JsString,
}
impl ModuleBinding {
    pub(super) fn trace(&self, tracer: &mut Tracer<'_>) {
        self.module.trace(tracer);
    }
    pub(super) fn call(&self, owner: &Object) -> RuntimeResult<Value> {
        self.module
            .load(owner)?
            .environment()
            .get_binding_value(&self.binding, true)
    }
}

type ResolveCallback = dyn Fn(&ModuleRequest) -> RuntimeResult<String>;
type NormalizeCallback = dyn Fn(Option<&JsString>, &ModuleRequest) -> RuntimeResult<JsString>;

/// Host-controlled module graph loader with per-specifier caching.
#[derive(Clone)]
pub struct ModuleGraph {
    realm: Realm,
    resolve: Rc<ResolveCallback>,
    normalize: Option<Rc<NormalizeCallback>>,
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
            normalize: None,
            modules: Rc::new(RefCell::new(Vec::new())),
        }
    }

    /// Creates a graph with referrer-aware canonical module identities.
    ///
    /// Normalization runs before the cache lookup. The source callback receives
    /// the canonical specifier; dependency records retain the original request.
    /// Import attributes remain part of the cache key.
    pub fn with_resolver(
        realm: Realm,
        normalize: impl Fn(Option<&JsString>, &ModuleRequest) -> RuntimeResult<JsString> + 'static,
        source: impl Fn(&ModuleRequest) -> RuntimeResult<String> + 'static,
    ) -> Self {
        Self {
            normalize: Some(Rc::new(normalize)),
            ..Self::new(realm, source)
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
        self.load_request(&request, None)
    }

    fn load_request(
        &self,
        request: &ModuleRequest,
        referrer: Option<&JsString>,
    ) -> RuntimeResult<SourceTextModule> {
        let normalized;
        let request = if let Some(normalize) = &self.normalize {
            normalized = ModuleRequest {
                specifier: normalize(referrer, request)?.code_units().to_vec(),
                ..request.clone()
            };
            &normalized
        } else {
            request
        };
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
        let program = parse_module(&source).map_err(|error| {
            let message = if self.normalize.is_some() {
                format!("{}: {error}", specifier.to_string_lossy())
            } else {
                error.to_string()
            };
            RuntimeError::syntax_error(message)
        })?;
        let requests = program.module_requests();
        let module = SourceTextModule::new(specifier.clone(), self.realm.clone(), program)?;
        self.modules.borrow_mut().push((key, module.clone()));
        let mut dependencies = Vec::new();
        for request in requests {
            dependencies.push(Dependency {
                module: StoredModule::store(
                    &module,
                    &self.load_request(&request, Some(&specifier))?,
                )?,
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

#[cfg(test)]
mod managed_tests {
    use super::*;
    fn graph(runtime: &Runtime, sources: &[(&str, &str)]) -> (Realm, ModuleGraph) {
        runtime.set_allocation_collection_interval(0);
        let realm = Realm::ordinary_in(runtime);
        let callback_runtime = runtime.clone();
        let callback = realm
            .create_native_function("collectModule", 0, move |_, _| {
                callback_runtime.verify_migration_barriers().unwrap();
                callback_runtime.collect_migration_graph().unwrap();
                Ok(Value::Undefined)
            })
            .unwrap();
        realm
            .global_object()
            .create_data_property("collectModule".into(), Value::Object(callback))
            .unwrap();
        let sources: Vec<_> = sources
            .iter()
            .map(|(name, source)| (name.to_string(), source.to_string()))
            .collect();
        let resolver_runtime = runtime.clone();
        let graph = ModuleGraph::new(realm.clone(), move |request| {
            resolver_runtime.verify_migration_barriers().unwrap();
            resolver_runtime.collect_migration_graph().unwrap();
            let name = String::from_utf16_lossy(&request.specifier);
            sources
                .iter()
                .find(|(candidate, _)| candidate == &name)
                .map(|(_, source)| source.clone())
                .ok_or_else(|| RuntimeError::reference_error("missing module"))
        });
        (realm, graph)
    }
    fn weak_data(module: &SourceTextModule) -> Weak<ModuleData> {
        match &module.0 {
            ModuleReference::Managed(root) => Rc::downgrade(&root.data),
            _ => panic!("managed module"),
        }
    }

    #[test]
    fn cyclic_module_records_share_facade_roots_and_reclaim_without_the_graph() {
        let runtime = Runtime::new();
        let (realm, graph) = graph(
            &runtime,
            &[
                (
                    "a",
                    "import {getB} from 'b';export function getA(){return 20;}export function answer(){collectModule();return getB();}",
                ),
                (
                    "b",
                    "import {getA} from 'a';export function getB(){return getA()+22;}",
                ),
            ],
        );
        let module = graph.load("a").unwrap();
        let clone = graph.load("a").unwrap();
        module.link().unwrap();
        module.evaluate().unwrap();
        runtime.verify_migration_barriers().unwrap();
        let weak = weak_data(&module);
        drop((realm, graph, module));
        assert_eq!(runtime.diagnostics().rooted, 1);
        runtime.collect_migration_graph().unwrap();
        let Value::Object(answer) = clone.get_export(&"answer".into()).unwrap() else {
            panic!("function")
        };
        assert!(
            answer
                .call(Value::Undefined, &[])
                .unwrap()
                .same_value(&Value::Number(42.0))
        );
        drop((answer, clone));
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn escaped_namespace_keeps_cyclic_records_and_live_bindings() {
        let runtime = Runtime::new();
        let (realm, graph) = graph(
            &runtime,
            &[
                (
                    "a",
                    "import * as dep from 'b';export {dep};export let value=20;collectModule();",
                ),
                (
                    "b",
                    "import {value} from 'a';export function read(){collectModule();return value+22;}",
                ),
            ],
        );
        let module = graph.load("a").unwrap();
        module.link().unwrap();
        module.evaluate().unwrap();
        let namespace = module.namespace_object().unwrap();
        assert_eq!(namespace, module.namespace_object().unwrap());
        assert!(namespace.prototype().is_none());
        assert!(!namespace.is_extensible());
        let weak = weak_data(&module);
        runtime.verify_migration_barriers().unwrap();
        drop((module, graph, realm));
        assert_eq!(runtime.diagnostics().rooted, 1);
        runtime.collect_migration_graph().unwrap();
        let read = || {
            let Value::Object(dep) = namespace.get(&"dep".into()).unwrap() else {
                panic!("namespace")
            };
            let Value::Object(read) = dep.get(&"read".into()).unwrap() else {
                panic!("read")
            };
            read.call(Value::Undefined, &[]).unwrap()
        };
        assert!(read().same_value(&Value::Number(42.0)));
        let owner = namespace
            .0
            .state
            .borrow()
            .module_namespace
            .as_ref()
            .unwrap()
            .load(&namespace)
            .unwrap();
        owner
            .environment()
            .set_mutable_binding(&"value".into(), Value::Number(21.0), true)
            .unwrap();
        assert!(read().same_value(&Value::Number(43.0)));
        drop(owner);
        assert!(weak.upgrade().is_some());
        drop(namespace);
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn empty_namespaces_retain_their_owner_without_getter_edges() {
        let runtime = Runtime::new();
        let (realm, graph) = graph(&runtime, &[("empty", "")]);
        let module = graph.load("empty").unwrap();
        module.link().unwrap();
        let weak = weak_data(&module);
        let namespace = module.namespace_object().unwrap();
        assert!(namespace.own_property_keys().is_empty());
        drop((module, graph, realm));
        runtime.collect_migration_graph().unwrap();
        assert!(weak.upgrade().is_some());
        drop(namespace);
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn cached_evaluation_promises_preserve_identity_and_escape_module_lifetime() {
        let runtime = Runtime::new();
        let (realm, graph) = graph(&runtime, &[("value", "collectModule();({answer:42});")]);
        let module = graph.load("value").unwrap();
        module.link().unwrap();
        let promise = module.evaluate().unwrap();
        let weak = promise.object().downgrade();
        drop(promise);
        runtime.verify_migration_barriers().unwrap();
        runtime.collect_migration_graph().unwrap();
        let promise = module.evaluate().unwrap();
        assert_eq!(Some(promise.object()), weak.upgrade());
        let module_weak = weak_data(&module);
        drop((module, graph, realm));
        runtime.collect_migration_graph().unwrap();
        assert!(module_weak.upgrade().is_none());
        let Value::Object(value) = promise.await_result().unwrap() else {
            panic!("result")
        };
        assert!(
            value
                .get(&"answer".into())
                .unwrap()
                .same_value(&Value::Number(42.0))
        );
        drop((value, promise));
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
    }

    #[test]
    fn cached_module_errors_keep_payload_identity_without_owning_roots() {
        let runtime = Runtime::new();
        let (realm, graph) = graph(&runtime, &[("error", "collectModule();throw {answer:42};")]);
        let module = graph.load("error").unwrap();
        module.link().unwrap();
        let error = module.evaluate().unwrap_err();
        let Some(Value::Object(value)) = error.thrown_value() else {
            panic!("payload")
        };
        let weak = value.downgrade();
        drop(error);
        runtime.verify_migration_barriers().unwrap();
        runtime.collect_migration_graph().unwrap();
        let error = module.evaluate().unwrap_err();
        assert!(
            matches!(error.thrown_value(), Some(Value::Object(value)) if Some(value.clone()) == weak.upgrade())
        );
        let module_weak = weak_data(&module);
        drop((module, graph, realm));
        runtime.collect_migration_graph().unwrap();
        assert!(module_weak.upgrade().is_none());
        let Some(Value::Object(value)) = error.thrown_value() else {
            panic!("payload")
        };
        assert!(
            value
                .get(&"answer".into())
                .unwrap()
                .same_value(&Value::Number(42.0))
        );
        drop(error);
        assert_eq!(runtime.collect_migration_graph().unwrap().retained, 0);
        assert!(weak.upgrade().is_none());
    }
}

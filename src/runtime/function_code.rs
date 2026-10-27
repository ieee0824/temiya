//! Share syntax within one Script without sharing closure environments.
use std::{cell::RefCell, collections::HashMap, ops::Deref, rc::Rc};

use crate::ast::{Function, Program};

type CodeEntries = HashMap<usize, Rc<Function>>;

#[derive(Debug)]
struct ScriptCode {
    program: Program,
    entries: RefCell<CodeEntries>,
}

#[derive(Clone, Debug)]
pub(super) struct FunctionCodeCache(Rc<ScriptCode>);

impl FunctionCodeCache {
    pub(super) fn new(program: Program) -> Self {
        Self(Rc::new(ScriptCode {
            program,
            entries: RefCell::new(HashMap::new()),
        }))
    }

    pub(super) fn program(&self) -> &Program {
        &self.0.program
    }

    pub(super) fn share(&self, code: &Function) -> FunctionCode {
        // Only immutable definitions borrowed from this owned Program or an
        // already cached Function may reach this method. The Program and every
        // cached Function stay alive for the cache's entire lifetime, so these
        // addresses cannot be recycled. Spans need not be unique. This key is
        // never dereferenced. Copied generator bodies use the uncached path.
        let key = std::ptr::from_ref(code) as usize;
        if let Some(previous) = self.0.entries.borrow().get(&key) {
            return FunctionCode::Shared(previous.clone());
        }
        let shared = Rc::new(code.clone());
        self.0.entries.borrow_mut().insert(key, shared.clone());
        FunctionCode::Shared(shared)
    }
}

#[derive(Clone, Debug)]
pub(super) enum FunctionCode {
    // Direct public evaluators accept arbitrary ASTs and retain their existing
    // owned-code behavior without allocating a cache or extra syntax Rc.
    Owned(Function),
    Shared(Rc<Function>),
}

impl Deref for FunctionCode {
    type Target = Function;

    fn deref(&self) -> &Function {
        match self {
            Self::Owned(code) => code,
            Self::Shared(code) => code,
        }
    }
}

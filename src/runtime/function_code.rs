//! Share syntax within one Script without sharing closure environments.
use std::{cell::RefCell, collections::HashMap, ops::Deref, rc::Rc};

use crate::ast::Function;

type CodeEntries = HashMap<(usize, usize), Rc<Function>>;

#[derive(Clone, Debug, Default)]
pub(super) struct FunctionCodeCache(Rc<RefCell<CodeEntries>>);

impl FunctionCodeCache {
    pub(super) fn share(&self, code: &Function) -> FunctionCode {
        let key = (code.span.start, code.span.end);
        if let Some(previous) = self.0.borrow().get(&key) {
            // Public owned ASTs may contain colliding spans, and generator
            // continuations can copy nodes. Neither spans nor addresses alone
            // prove code identity. Check the complete immutable syntax.
            if previous.as_ref() == code {
                return FunctionCode::Shared(previous.clone());
            }
        }
        let shared = Rc::new(code.clone());
        self.0.borrow_mut().insert(key, shared.clone());
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

//! Private identities contain no JavaScript edges; values use traced storage.
use super::{
    Environment, JsString, Object, RuntimeError, RuntimeResult, Value, stored::StoredValue,
};
use std::rc::Rc;

// Sparse out-of-line payloads keep ordinary objects and environments small.
#[derive(Default)]
pub(super) struct Names(pub(super) Vec<(JsString, Name)>);
// The first private slot shares its allocation with the sparse payload. Larger
// classes spill into a vector without enlarging ordinary ObjectState records.
pub(super) struct Fields {
    first: (Name, StoredValue),
    rest: Vec<(Name, StoredValue)>,
}
impl Fields {
    fn new(name: Name, value: StoredValue) -> Self {
        Self {
            first: (name, value),
            rest: Vec::new(),
        }
    }
    pub(super) fn iter(&self) -> impl Iterator<Item = &(Name, StoredValue)> {
        std::iter::once(&self.first).chain(self.rest.iter())
    }
    fn iter_mut(&mut self) -> impl Iterator<Item = &mut (Name, StoredValue)> {
        std::iter::once(&mut self.first).chain(self.rest.iter_mut())
    }
    fn push(&mut self, name: Name, value: StoredValue) {
        if self.rest.len() == self.rest.capacity() {
            self.rest.reserve_exact(self.rest.len().max(1));
        }
        self.rest.push((name, value));
    }
}

#[derive(Clone, Debug)]
pub(super) struct Name(Rc<()>);
impl Name {
    fn new() -> Self {
        Self(Rc::new(()))
    }
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Environment {
    pub(super) fn declare_private(&self, name: JsString) {
        self.0
            .private_names
            .borrow_mut()
            .get_or_insert_with(Default::default)
            .0
            .push((name, Name::new()));
    }
    pub(super) fn resolve_private(&self, name: &str) -> RuntimeResult<Name> {
        self.resolve_private_key(&JsString::from(name))
    }
    pub(super) fn resolve_private_key(&self, name: &JsString) -> RuntimeResult<Name> {
        let mut environment = Some(self.clone());
        while let Some(current) = environment {
            if let Some(names) = current.0.private_names.borrow().as_ref() {
                if let Some((_, identity)) = names.0.iter().find(|(n, _)| n == name) {
                    return Ok(identity.clone());
                }
            }
            environment = current.outer();
        }
        Err(RuntimeError::syntax_error("unresolved private name"))
    }
}

impl Object {
    pub(super) fn has_private(&self, name: &Name) -> bool {
        self.0
            .state
            .borrow()
            .private_fields
            .as_ref()
            .is_some_and(|fields| fields.iter().any(|(key, _)| key.same(name)))
    }
    pub(super) fn add_private(&self, name: Name, value: Value) -> RuntimeResult<()> {
        if self.has_private(&name) {
            return Err(RuntimeError::type_error(
                "private field already initialized",
            ));
        }
        let stored = StoredValue::store_owned(self, value)?;
        let mut state = self.0.state.borrow_mut();
        match &mut state.private_fields {
            Some(fields) => fields.push(name, stored),
            None => state.private_fields = Some(Box::new(Fields::new(name, stored))),
        }
        Ok(())
    }
    pub(super) fn get_private(&self, name: &Name) -> RuntimeResult<Value> {
        let stored = self
            .0
            .state
            .borrow()
            .private_fields
            .as_ref()
            .and_then(|fields| fields.iter().find(|(key, _)| key.same(name)))
            .map(|(_, value)| value.clone())
            .ok_or_else(|| {
                RuntimeError::type_error("private field receiver has no matching brand")
            })?;
        stored.load(self)
    }
    pub(super) fn set_private(&self, name: &Name, value: Value) -> RuntimeResult<()> {
        if !self.has_private(name) {
            return Err(RuntimeError::type_error(
                "private field receiver has no matching brand",
            ));
        }
        let stored = StoredValue::store_owned(self, value)?;
        let mut state = self.0.state.borrow_mut();
        let (_, slot) = state
            .private_fields
            .as_mut()
            .expect("checked brand")
            .iter_mut()
            .find(|(key, _)| key.same(name))
            .expect("checked field");
        *slot = stored;
        Ok(())
    }
}

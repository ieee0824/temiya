//! Private identities contain no JavaScript edges; values use traced storage.
use super::{
    Environment, JsString, Object, RuntimeError, RuntimeResult, Value, stored::StoredValue,
};
use std::rc::Rc;

// Sparse out-of-line payloads keep ordinary objects and environments small.
#[derive(Default)]
pub(super) struct Names(pub(super) Vec<(JsString, Name)>);
#[derive(Default)]
pub(super) struct Fields(pub(super) Vec<(Name, StoredValue)>);

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
        let key = JsString::from(name);
        let mut environment = Some(self.clone());
        while let Some(current) = environment {
            if let Some(names) = current.0.private_names.borrow().as_ref() {
                if let Some((_, identity)) = names.0.iter().find(|(n, _)| n == &key) {
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
            .is_some_and(|fields| fields.0.iter().any(|(key, _)| key.same(name)))
    }
    pub(super) fn add_private(&self, name: Name, value: Value) -> RuntimeResult<()> {
        if self.has_private(&name) {
            return Err(RuntimeError::type_error(
                "private field already initialized",
            ));
        }
        let stored = StoredValue::store_owned(self, value)?;
        self.0
            .state
            .borrow_mut()
            .private_fields
            .get_or_insert_with(Default::default)
            .0
            .push((name, stored));
        Ok(())
    }
    pub(super) fn get_private(&self, name: &Name) -> RuntimeResult<Value> {
        let stored = self
            .0
            .state
            .borrow()
            .private_fields
            .as_ref()
            .and_then(|fields| fields.0.iter().find(|(key, _)| key.same(name)))
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
            .0
            .iter_mut()
            .find(|(key, _)| key.same(name))
            .expect("checked field");
        *slot = stored;
        Ok(())
    }
}

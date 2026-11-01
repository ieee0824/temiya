//! Compact optional state; uncommon combinations retain all independent slots.
use super::{JsString, ObjectState, Value};
use std::rc::Rc;

macro_rules! payload_slots {
    ($( $variant:ident => $get:ident, $get_mut:ident : $ty:ty; )*) => {
        #[derive(Default)]
        pub(super) struct MultiplePayload { $( $get: Option<$ty>, )* }
        #[derive(Default)]
        pub(super) enum ObjectPayload {
            #[default]
            Empty,
            $( $variant(Option<$ty>), )*
            Multiple(Box<MultiplePayload>),
        }
        impl ObjectPayload {
            fn promote(&mut self) {
                let mut slots = MultiplePayload::default();
                match std::mem::take(self) {
                    Self::Empty => {},
                    $( Self::$variant(value) => slots.$get = value, )*
                    Self::Multiple(slots) => {
                        *self = Self::Multiple(slots);
                        return;
                    }
                }
                *self = Self::Multiple(Box::new(slots));
            }
        }
        impl ObjectState {
            $(
                #[inline]
                pub(in crate::runtime) fn $get(&self) -> &Option<$ty> {
                    match &self.payload {
                        ObjectPayload::$variant(value) => value,
                        ObjectPayload::Multiple(slots) => &slots.$get,
                        _ => &None,
                    }
                }
                #[inline]
                pub(in crate::runtime) fn $get_mut(&mut self) -> &mut Option<$ty> {
                    if matches!(self.payload, ObjectPayload::Empty) {
                        self.payload = ObjectPayload::$variant(None);
                    }
                    if !matches!(self.payload, ObjectPayload::$variant(_) | ObjectPayload::Multiple(_)) {
                        self.payload.promote();
                    }
                    match &mut self.payload {
                        ObjectPayload::$variant(value) => value,
                        ObjectPayload::Multiple(slots) => &mut slots.$get,
                        _ => unreachable!("payload slot selected before mutable access"),
                    }
                }
            )*
        }
    };
}

payload_slots! {
    Buffer => buffer, buffer_mut: super::super::indexed::BufferSlot;
    View => view, view_mut: Box<super::super::indexed::ViewSlot>;
    StringData => string_data, string_data_mut: Box<JsString>;
    Regexp => regexp, regexp_mut: super::super::regexp::RegExpSlot;
    PrimitiveData => primitive_data, primitive_data_mut: Box<Value>;
    MapData => map_data, map_data_mut: super::super::collections::MapSlot;
    SetData => set_data, set_data_mut: super::super::collections::SetSlot;
    WeakMapData => weak_map_data, weak_map_data_mut: super::super::collections::WeakMapSlot;
    WeakSetData => weak_set_data, weak_set_data_mut: super::super::collections::WeakSetSlot;
    PromiseData => promise_data, promise_data_mut: Rc<std::cell::RefCell<super::super::promise::PromiseData>>;
    GeneratorData => generator_data, generator_data_mut: Rc<std::cell::RefCell<super::super::generator::GeneratorSlot>>;
    ModuleNamespace => module_namespace, module_namespace_mut: Box<super::super::pipeline::StoredModule>;
    DateValue => date_value, date_value_mut: f64;
    Iterator => iterator, iterator_mut: Box<super::super::iterator::IteratorSlot>;
    Proxy => proxy, proxy_mut: Box<super::super::proxy::ProxySlot>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{Object, Runtime, stored::StoredValue};
    use std::cell::RefCell;

    #[test]
    fn multiple_payload_slots_keep_edges_and_release_removed_values() {
        let runtime = Runtime::new();
        let owner = Object::new_in(&runtime);
        let child = Object::new_in(&runtime);
        let weak = child.downgrade();
        let value = StoredValue::store_owned(&owner, Value::Object(child)).unwrap();
        *owner.0.state.borrow_mut().map_data_mut() = Some(Rc::new(RefCell::new(vec![Some((
            StoredValue::Number(1.0),
            value,
        ))])));
        assert_eq!(runtime.collect_garbage().unwrap().retained, 2);
        *owner.0.state.borrow_mut().buffer_mut() = Some(Rc::new(RefCell::new(Some(vec![1, 2, 3]))));
        assert_eq!(runtime.collect_garbage().unwrap().retained, 2);
        assert!(weak.upgrade().is_some());
        {
            let state = owner.0.state.borrow();
            assert_eq!(
                state.buffer().as_ref().unwrap().borrow().as_deref(),
                Some(&[1, 2, 3][..])
            );
            assert!(state.map_data().is_some());
        }
        *owner.0.state.borrow_mut().map_data_mut() = None;
        assert_eq!(runtime.collect_garbage().unwrap().retained, 1);
        assert!(weak.upgrade().is_none());
        assert_eq!(
            owner
                .0
                .state
                .borrow()
                .buffer()
                .as_ref()
                .unwrap()
                .borrow()
                .as_ref()
                .unwrap()
                .len(),
            3
        );
        drop(owner);
        assert_eq!(runtime.collect_garbage().unwrap().retained, 0);
    }
}

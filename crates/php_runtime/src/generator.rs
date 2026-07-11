//! Internal generator runtime state for runtime-semantics.

use crate::{ObjectRef, Value};
use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_GENERATOR_ID: AtomicU64 = AtomicU64::new(1);

/// Generator lifecycle state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeneratorState {
    /// Function call returned a generator object but the body has not run.
    Created,
    /// Generator body is currently executing.
    Running,
    /// Generator stopped at a `yield`.
    Suspended,
    /// Generator completed normally or was closed by the VM.
    Closed,
    /// Generator errored while executing.
    Errored,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct GeneratorStorage {
    function: u32,
    args: Vec<Value>,
    call_context: GeneratorCallContext,
    state: GeneratorState,
    current_key: Option<Value>,
    current_value: Option<Value>,
    return_value: Option<Value>,
}

/// Activation context captured when a generator object is created.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GeneratorCallContext {
    pub this_value: Option<ObjectRef>,
    pub scope_class: Option<Arc<str>>,
    pub called_class: Option<Arc<str>>,
    pub declaring_class: Option<Arc<str>>,
    pub call_site_strict_types: Option<bool>,
}

/// Internal reference to generator state.
#[derive(Clone)]
struct GeneratorCell {
    id: u64,
    storage: RefCell<GeneratorStorage>,
}

#[derive(Clone)]
pub struct GeneratorRef {
    cell: Rc<GeneratorCell>,
}

impl GeneratorRef {
    /// Creates a generator in the `Created` state.
    #[must_use]
    pub fn new(function: u32, args: Vec<Value>) -> Self {
        Self::new_with_context(function, args, GeneratorCallContext::default())
    }

    /// Creates a generator with its activation context captured.
    #[must_use]
    pub fn new_with_context(
        function: u32,
        args: Vec<Value>,
        call_context: GeneratorCallContext,
    ) -> Self {
        Self {
            cell: Rc::new(GeneratorCell {
                id: NEXT_GENERATOR_ID.fetch_add(1, Ordering::Relaxed),
                storage: RefCell::new(GeneratorStorage {
                    function,
                    args,
                    call_context,
                    state: GeneratorState::Created,
                    current_key: None,
                    current_value: None,
                    return_value: None,
                }),
            }),
        }
    }

    /// Stable debug identity.
    #[must_use]
    pub fn id(&self) -> u64 {
        self.cell.id
    }

    /// Raw IR function ID.
    #[must_use]
    pub fn function(&self) -> u32 {
        self.cell.storage.borrow().function
    }

    /// Positional argument snapshots.
    #[must_use]
    pub fn args(&self) -> Vec<Value> {
        self.cell.storage.borrow().args.clone()
    }

    /// Captured activation context for the first generator resume.
    #[must_use]
    pub fn call_context(&self) -> GeneratorCallContext {
        self.cell.storage.borrow().call_context.clone()
    }

    /// Current lifecycle state.
    #[must_use]
    pub fn state(&self) -> GeneratorState {
        self.cell.storage.borrow().state
    }

    /// Sets the lifecycle state.
    pub fn set_state(&self, state: GeneratorState) {
        self.cell.storage.borrow_mut().state = state;
    }

    /// Records the current yielded key/value and marks the generator suspended.
    pub fn suspend(&self, key: Option<Value>, value: Value) {
        let mut storage = self.cell.storage.borrow_mut();
        storage.current_key = key;
        storage.current_value = Some(value);
        storage.state = GeneratorState::Suspended;
    }

    /// Marks the generator as completed.
    pub fn close(&self, return_value: Option<Value>) {
        let mut storage = self.cell.storage.borrow_mut();
        storage.current_key = None;
        storage.current_value = None;
        storage.return_value = return_value;
        storage.state = GeneratorState::Closed;
    }

    /// Current yielded key/value, if any.
    #[must_use]
    pub fn current(&self) -> Option<(Option<Value>, Value)> {
        let storage = self.cell.storage.borrow();
        storage
            .current_value
            .clone()
            .map(|value| (storage.current_key.clone(), value))
    }

    /// Current yielded key, if any.
    #[must_use]
    pub fn current_key(&self) -> Option<Value> {
        self.cell.storage.borrow().current_key.clone()
    }

    /// Current yielded value, if any.
    #[must_use]
    pub fn current_value(&self) -> Option<Value> {
        self.cell.storage.borrow().current_value.clone()
    }

    /// Return value recorded after normal completion.
    #[must_use]
    pub fn return_value(&self) -> Option<Value> {
        self.cell.storage.borrow().return_value.clone()
    }
}

impl fmt::Debug for GeneratorRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GeneratorRef")
            .field("id", &self.cell.id)
            .field("function", &self.function())
            .field("state", &self.state())
            .finish()
    }
}

impl PartialEq for GeneratorRef {
    fn eq(&self, other: &Self) -> bool {
        self.cell.id == other.cell.id
    }
}

impl Eq for GeneratorRef {}

#[cfg(test)]
mod tests {
    use super::{GeneratorRef, GeneratorState};
    use crate::Value;

    #[test]
    fn generator_state_transitions_are_explicit() {
        let generator = GeneratorRef::new(7, vec![Value::Int(1)]);

        assert_eq!(generator.state(), GeneratorState::Created);
        assert_eq!(generator.args(), vec![Value::Int(1)]);

        generator.set_state(GeneratorState::Running);
        assert_eq!(generator.state(), GeneratorState::Running);

        generator.suspend(Some(Value::Int(2)), Value::string(b"value".to_vec()));
        assert_eq!(generator.state(), GeneratorState::Suspended);
        assert_eq!(
            generator.current(),
            Some((Some(Value::Int(2)), Value::string(b"value".to_vec())))
        );

        generator.close(Some(Value::Int(9)));
        assert_eq!(generator.state(), GeneratorState::Closed);
        assert_eq!(generator.current(), None);
        assert_eq!(generator.return_value(), Some(Value::Int(9)));
    }
}

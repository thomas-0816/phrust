//! Internal fiber runtime state for runtime-semantics.

use crate::Value;
use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_FIBER_ID: AtomicU64 = AtomicU64::new(1);

/// Fiber lifecycle state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FiberState {
    /// Fiber object was constructed but has not started.
    NotStarted,
    /// Fiber callable is currently executing.
    Running,
    /// Fiber stopped at `Fiber::suspend`.
    Suspended,
    /// Fiber completed normally.
    Terminated,
    /// Fiber callable errored while executing.
    Errored,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FiberStorage {
    callable: Value,
    state: FiberState,
    return_value: Option<Value>,
}

/// Internal reference to fiber state.
#[derive(Clone)]
struct FiberCell {
    id: u64,
    storage: RefCell<FiberStorage>,
}

#[derive(Clone)]
pub struct FiberRef {
    cell: Rc<FiberCell>,
}

impl FiberRef {
    /// Creates a fiber in the `NotStarted` state.
    #[must_use]
    pub fn new(callable: Value) -> Self {
        Self {
            cell: Rc::new(FiberCell {
                id: NEXT_FIBER_ID.fetch_add(1, Ordering::Relaxed),
                storage: RefCell::new(FiberStorage {
                    callable,
                    state: FiberState::NotStarted,
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

    /// Callable snapshot supplied to the constructor.
    #[must_use]
    pub fn callable(&self) -> Value {
        self.cell.storage.borrow().callable.clone()
    }

    /// Current lifecycle state.
    #[must_use]
    pub fn state(&self) -> FiberState {
        self.cell.storage.borrow().state
    }

    /// Sets the lifecycle state.
    pub fn set_state(&self, state: FiberState) {
        self.cell.storage.borrow_mut().state = state;
    }

    /// Marks the fiber as completed normally.
    pub fn terminate(&self, return_value: Option<Value>) {
        let mut storage = self.cell.storage.borrow_mut();
        storage.return_value = return_value;
        storage.state = FiberState::Terminated;
    }

    /// Return value recorded after normal completion.
    #[must_use]
    pub fn return_value(&self) -> Option<Value> {
        self.cell.storage.borrow().return_value.clone()
    }
}

impl fmt::Debug for FiberRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FiberRef")
            .field("id", &self.cell.id)
            .field("state", &self.state())
            .finish()
    }
}

impl PartialEq for FiberRef {
    fn eq(&self, other: &Self) -> bool {
        self.cell.id == other.cell.id
    }
}

impl Eq for FiberRef {}

#[cfg(test)]
mod tests {
    use super::{FiberRef, FiberState};
    use crate::Value;

    #[test]
    fn fiber_state_transitions_are_explicit() {
        let fiber = FiberRef::new(Value::internal_builtin_callable("strlen"));

        assert_eq!(fiber.state(), FiberState::NotStarted);
        assert!(matches!(fiber.callable(), Value::Callable(_)));

        fiber.set_state(FiberState::Running);
        assert_eq!(fiber.state(), FiberState::Running);

        fiber.set_state(FiberState::Suspended);
        assert_eq!(fiber.state(), FiberState::Suspended);

        fiber.terminate(Some(Value::Int(7)));
        assert_eq!(fiber.state(), FiberState::Terminated);
        assert_eq!(fiber.return_value(), Some(Value::Int(7)));

        fiber.set_state(FiberState::Errored);
        assert_eq!(fiber.state(), FiberState::Errored);
    }
}

//! Typed request-local extension state with registration-time slot assignment.

use std::any::{Any, TypeId, type_name};
use std::fmt;
use std::marker::PhantomData;
use std::mem::size_of;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_LAYOUT_ID: AtomicU64 = AtomicU64::new(1);

/// Stable numeric slot assigned while an extension-state layout is assembled.
pub struct ExtensionStateSlot<T: 'static> {
    layout_id: u64,
    index: usize,
    marker: PhantomData<fn() -> T>,
}

/// Registration-time slot metadata before an extension binds its concrete type.
#[derive(Clone, Copy, Debug)]
pub struct ErasedExtensionStateSlot {
    layout_id: u64,
    index: usize,
    type_id: TypeId,
}

impl ErasedExtensionStateSlot {
    /// Zero-based slot index fixed for the lifetime of the registry layout.
    #[must_use]
    pub const fn index(self) -> usize {
        self.index
    }

    /// Binds registration metadata to the expected concrete extension state.
    #[must_use]
    pub fn typed<T: Any>(self) -> Option<ExtensionStateSlot<T>> {
        (self.type_id == TypeId::of::<T>()).then_some(ExtensionStateSlot {
            layout_id: self.layout_id,
            index: self.index,
            marker: PhantomData,
        })
    }
}

impl<T: 'static> ExtensionStateSlot<T> {
    /// Zero-based slot index used by the request hot path.
    #[must_use]
    pub const fn index(self) -> usize {
        self.index
    }
}

impl<T: 'static> Clone for ExtensionStateSlot<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static> Copy for ExtensionStateSlot<T> {}

impl<T: 'static> fmt::Debug for ExtensionStateSlot<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExtensionStateSlot")
            .field("type", &type_name::<T>())
            .field("index", &self.index)
            .finish()
    }
}

struct StateRegistration {
    type_id: TypeId,
    type_name: &'static str,
    payload_bytes: usize,
    create: Arc<dyn Fn() -> Box<dyn Any> + Send + Sync>,
}

impl fmt::Debug for StateRegistration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StateRegistration")
            .field("type_name", &self.type_name)
            .field("payload_bytes", &self.payload_bytes)
            .finish_non_exhaustive()
    }
}

/// Registration error that keeps ambiguous layouts from reaching requests.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExtensionStateLayoutError {
    DuplicateType(&'static str),
}

/// Builder used once while an engine or extension registry is assembled.
#[derive(Debug)]
pub struct ExtensionStateLayoutBuilder {
    layout_id: u64,
    registrations: Vec<StateRegistration>,
}

impl Default for ExtensionStateLayoutBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ExtensionStateLayoutBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self {
            layout_id: NEXT_LAYOUT_ID.fetch_add(1, Ordering::Relaxed),
            registrations: Vec::new(),
        }
    }

    /// Registers one state type and returns its direct typed slot.
    pub fn register<T, F>(
        &mut self,
        create: F,
    ) -> Result<ExtensionStateSlot<T>, ExtensionStateLayoutError>
    where
        T: Any,
        F: Fn() -> T + Send + Sync + 'static,
    {
        if self
            .registrations
            .iter()
            .any(|registration| registration.type_id == TypeId::of::<T>())
        {
            return Err(ExtensionStateLayoutError::DuplicateType(type_name::<T>()));
        }
        let index = self.registrations.len();
        self.registrations.push(StateRegistration {
            type_id: TypeId::of::<T>(),
            type_name: type_name::<T>(),
            payload_bytes: size_of::<T>(),
            create: Arc::new(move || Box::new(create())),
        });
        Ok(ExtensionStateSlot {
            layout_id: self.layout_id,
            index,
            marker: PhantomData,
        })
    }

    /// Registers an extension descriptor's erased factory during registry assembly.
    pub fn register_factory(
        &mut self,
        type_id: TypeId,
        type_name: &'static str,
        payload_bytes: usize,
        create: fn() -> Box<dyn Any>,
    ) -> Result<ErasedExtensionStateSlot, ExtensionStateLayoutError> {
        if self
            .registrations
            .iter()
            .any(|registration| registration.type_id == type_id)
        {
            return Err(ExtensionStateLayoutError::DuplicateType(type_name));
        }
        let index = self.registrations.len();
        self.registrations.push(StateRegistration {
            type_id,
            type_name,
            payload_bytes,
            create: Arc::new(create),
        });
        Ok(ErasedExtensionStateSlot {
            layout_id: self.layout_id,
            index,
            type_id,
        })
    }

    /// Freezes registration order for request allocation.
    #[must_use]
    pub fn build(self) -> ExtensionStateLayout {
        ExtensionStateLayout {
            layout_id: self.layout_id,
            registrations: self.registrations.into(),
        }
    }
}

/// Immutable request-state layout shared by an engine and its requests.
#[derive(Clone, Debug)]
pub struct ExtensionStateLayout {
    layout_id: u64,
    registrations: Arc<[StateRegistration]>,
}

impl ExtensionStateLayout {
    #[must_use]
    pub fn slot_count(&self) -> usize {
        self.registrations.len()
    }

    /// Sum of concrete state payload sizes, excluding allocator overhead.
    #[must_use]
    pub fn payload_bytes(&self) -> usize {
        self.registrations
            .iter()
            .map(|registration| registration.payload_bytes)
            .sum()
    }

    /// Allocates exactly the states registered in this layout.
    #[must_use]
    pub fn create_request_state(&self) -> RequestState {
        RequestState {
            layout_id: self.layout_id,
            values: self
                .registrations
                .iter()
                .map(|registration| (registration.create)())
                .collect(),
        }
    }
}

/// Sole owner of extension state for one PHP request.
#[derive(Debug)]
pub struct RequestState {
    layout_id: u64,
    values: Vec<Box<dyn Any>>,
}

impl RequestState {
    /// Returns a state by its registration-time slot without name lookup.
    #[must_use]
    pub fn get<T: Any>(&self, slot: ExtensionStateSlot<T>) -> Option<&T> {
        self.matches(slot)
            .then(|| self.values.get(slot.index))
            .flatten()
            .and_then(|value| value.downcast_ref())
    }

    /// Returns mutable state by its registration-time slot without allocation.
    pub fn get_mut<T: Any>(&mut self, slot: ExtensionStateSlot<T>) -> Option<&mut T> {
        if !self.matches(slot) {
            return None;
        }
        self.values
            .get_mut(slot.index)
            .and_then(|value| value.downcast_mut())
    }

    /// Safely borrows two distinct registered states at once.
    pub fn get_pair_mut<A: Any, B: Any>(
        &mut self,
        first: ExtensionStateSlot<A>,
        second: ExtensionStateSlot<B>,
    ) -> Option<(&mut A, &mut B)> {
        if !self.matches(first) || !self.matches(second) || first.index == second.index {
            return None;
        }
        if first.index < second.index {
            let (left, right) = self.values.split_at_mut(second.index);
            Some((left[first.index].downcast_mut()?, right[0].downcast_mut()?))
        } else {
            let (left, right) = self.values.split_at_mut(first.index);
            Some((right[0].downcast_mut()?, left[second.index].downcast_mut()?))
        }
    }

    #[must_use]
    pub fn slot_count(&self) -> usize {
        self.values.len()
    }

    fn matches<T: 'static>(&self, slot: ExtensionStateSlot<T>) -> bool {
        self.layout_id == slot.layout_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn allocates_only_registered_states_and_uses_numeric_slots() {
        let mut builder = ExtensionStateLayoutBuilder::new();
        let number = builder.register(|| 7_u64).expect("number slot");
        let layout = builder.build();
        let mut request = layout.create_request_state();

        assert_eq!(number.index(), 0);
        assert_eq!(layout.slot_count(), 1);
        assert_eq!(request.slot_count(), 1);
        assert_eq!(request.get(number), Some(&7));
        *request.get_mut(number).expect("registered number") = 9;
        assert_eq!(request.get(number), Some(&9));
    }

    #[test]
    fn layouts_reject_duplicate_types_and_cross_layout_slots() {
        let mut first_builder = ExtensionStateLayoutBuilder::new();
        let first = first_builder.register(|| 1_u8).expect("first slot");
        assert!(matches!(
            first_builder.register(|| 2_u8),
            Err(ExtensionStateLayoutError::DuplicateType(name)) if name == type_name::<u8>()
        ));
        let first_request = first_builder.build().create_request_state();

        let mut second_builder = ExtensionStateLayoutBuilder::new();
        let second = second_builder.register(|| 3_u8).expect("second slot");
        let second_request = second_builder.build().create_request_state();
        assert_eq!(first_request.get(second), None);
        assert_eq!(second_request.get(first), None);
    }

    #[test]
    fn erased_registration_binds_only_the_declared_concrete_type() {
        fn create_number() -> Box<dyn Any> {
            Box::new(11_u64)
        }

        let mut builder = ExtensionStateLayoutBuilder::new();
        let erased = builder
            .register_factory(
                TypeId::of::<u64>(),
                type_name::<u64>(),
                size_of::<u64>(),
                create_number,
            )
            .expect("erased slot");
        let number = erased.typed::<u64>().expect("matching type");
        assert!(erased.typed::<String>().is_none());
        let request = builder.build().create_request_state();
        assert_eq!(request.get(number), Some(&11));
    }

    #[test]
    fn pair_borrow_is_safe_and_request_owners_are_isolated() {
        let mut builder = ExtensionStateLayoutBuilder::new();
        let number = builder.register(|| 0_u64).expect("number slot");
        let text = builder.register(String::new).expect("text slot");
        let layout = builder.build();
        let mut first = layout.create_request_state();
        let second = layout.create_request_state();

        let (number_state, text_state) = first
            .get_pair_mut(number, text)
            .expect("distinct registered slots");
        *number_state = 4;
        text_state.push_str("first");

        assert_eq!(first.get(number), Some(&4));
        assert_eq!(first.get(text).map(String::as_str), Some("first"));
        assert_eq!(second.get(number), Some(&0));
        assert_eq!(second.get(text).map(String::as_str), Some(""));
    }

    #[test]
    fn dropping_request_state_drops_each_registered_value_once() {
        struct DropProbe(Arc<AtomicUsize>);
        impl Drop for DropProbe {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }

        let drops = Arc::new(AtomicUsize::new(0));
        let mut builder = ExtensionStateLayoutBuilder::new();
        let factory_drops = Arc::clone(&drops);
        builder
            .register(move || DropProbe(Arc::clone(&factory_drops)))
            .expect("drop slot");
        let layout = builder.build();
        let request = layout.create_request_state();
        assert_eq!(drops.load(Ordering::Relaxed), 0);
        drop(request);
        assert_eq!(drops.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn empty_layout_allocates_no_disabled_extension_state() {
        let layout = ExtensionStateLayoutBuilder::new().build();
        let request = layout.create_request_state();
        assert_eq!(layout.slot_count(), 0);
        assert_eq!(layout.payload_bytes(), 0);
        assert_eq!(request.slot_count(), 0);
    }

    #[test]
    fn concurrent_requests_create_isolated_state_on_their_worker_threads() {
        let mut builder = ExtensionStateLayoutBuilder::new();
        let number = builder.register(|| 0_u64).expect("number slot");
        let layout = builder.build();
        let handles = (1..=4)
            .map(|value| {
                let layout = layout.clone();
                std::thread::spawn(move || {
                    let mut request = layout.create_request_state();
                    *request.get_mut(number).expect("number state") = value;
                    *request.get(number).expect("number state")
                })
            })
            .collect::<Vec<_>>();

        let mut values = handles
            .into_iter()
            .map(|handle| handle.join().expect("request thread"))
            .collect::<Vec<_>>();
        values.sort_unstable();
        assert_eq!(values, vec![1, 2, 3, 4]);
    }

    #[test]
    fn request_state_drops_deterministically_during_unwind() {
        let drops = Arc::new(AtomicUsize::new(0));
        let mut builder = ExtensionStateLayoutBuilder::new();
        let factory_drops = Arc::clone(&drops);
        builder
            .register(move || DropProbeForUnwind(Arc::clone(&factory_drops)))
            .expect("drop slot");
        let layout = builder.build();

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _request = layout.create_request_state();
            panic!("simulated exception or timeout unwind");
        }));
        assert!(result.is_err());
        assert_eq!(drops.load(Ordering::Relaxed), 1);
    }

    struct DropProbeForUnwind(Arc<AtomicUsize>);

    impl Drop for DropProbeForUnwind {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
}

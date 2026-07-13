//! Statically linked extension implementations and integration registry.

mod apcu;
mod ctype;
mod generated;

use php_runtime::api::{BuiltinEntry, BuiltinRegistry as RuntimeBuiltinRegistry};
use php_runtime::api::{
    ErasedExtensionStateSlot, ExtensionStateLayout, ExtensionStateLayoutBuilder,
    ExtensionStateSlot, RequestState,
};
use php_runtime::api::{ExtensionDescriptor, ExtensionModule};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

static APCU: apcu::ApcuExtension = apcu::ApcuExtension;
static CTYPE: ctype::CtypeExtension = ctype::CtypeExtension;
static DEFAULT_MODULES: [&dyn ExtensionModule; 2] = [&APCU, &CTYPE];

/// Deterministic extension assembly failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExtensionRegistryError {
    DuplicateExtension(&'static str),
    DuplicateFunction(&'static str),
    DuplicateStateType(&'static str),
    MissingDependency {
        extension: &'static str,
        dependency: &'static str,
    },
    DependencyCycle,
}

/// Validated selected extension set.
#[derive(Debug)]
pub struct ExtensionRegistry {
    descriptors: Vec<&'static ExtensionDescriptor>,
    entries: Vec<BuiltinEntry>,
    request_state_layout: ExtensionStateLayout,
    request_state_slots: BTreeMap<&'static str, ErasedExtensionStateSlot>,
}

impl ExtensionRegistry {
    pub fn assemble(
        modules: &[&'static dyn ExtensionModule],
    ) -> Result<Self, ExtensionRegistryError> {
        let mut by_name = BTreeMap::new();
        for module in modules {
            let descriptor = module.descriptor();
            if by_name.insert(descriptor.name, descriptor).is_some() {
                return Err(ExtensionRegistryError::DuplicateExtension(descriptor.name));
            }
        }

        let mut ordered = Vec::with_capacity(by_name.len());
        let mut visiting = BTreeSet::new();
        let mut visited = BTreeSet::new();
        for name in by_name.keys().copied() {
            visit(name, &by_name, &mut visiting, &mut visited, &mut ordered)?;
        }

        let mut entries = RuntimeBuiltinRegistry::new().entries().to_vec();
        for descriptor in &ordered {
            entries.extend_from_slice(descriptor.functions);
        }
        entries.sort_unstable_by_key(|entry| entry.name());
        if let Some(pair) = entries
            .windows(2)
            .find(|pair| pair[0].name() == pair[1].name())
        {
            return Err(ExtensionRegistryError::DuplicateFunction(pair[0].name()));
        }

        let mut layout = ExtensionStateLayoutBuilder::new();
        let mut request_state_slots = BTreeMap::new();
        for descriptor in &ordered {
            let Some(factory) = descriptor.request_state else {
                continue;
            };
            let slot = layout
                .register_factory(
                    factory.type_id(),
                    factory.type_name(),
                    factory.payload_bytes(),
                    factory.create_fn(),
                )
                .map_err(|_| ExtensionRegistryError::DuplicateStateType(factory.type_name()))?;
            request_state_slots.insert(descriptor.name, slot);
        }
        Ok(Self {
            descriptors: ordered,
            entries,
            request_state_layout: layout.build(),
            request_state_slots,
        })
    }

    pub fn descriptors(&self) -> &[&'static ExtensionDescriptor] {
        &self.descriptors
    }

    pub fn entries(&self) -> &[BuiltinEntry] {
        &self.entries
    }

    pub fn get(&self, name: &str) -> Option<BuiltinEntry> {
        self.entries
            .binary_search_by(|entry| entry.name().cmp(name))
            .ok()
            .map(|index| self.entries[index])
    }

    pub fn contains(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// Allocates exactly the request-local states selected by this registry.
    pub fn create_request_state(&self) -> RequestState {
        self.request_state_layout.create_request_state()
    }

    /// Immutable layout assembled once with the selected extension set.
    pub fn request_state_layout(&self) -> &ExtensionStateLayout {
        &self.request_state_layout
    }

    /// Resolves a typed slot during engine setup, never during builtin dispatch.
    pub fn request_state_slot<T: 'static>(&self, extension: &str) -> Option<ExtensionStateSlot<T>> {
        self.request_state_slots
            .get(extension)
            .copied()
            .and_then(ErasedExtensionStateSlot::typed::<T>)
    }
}

fn visit(
    name: &'static str,
    modules: &BTreeMap<&'static str, &'static ExtensionDescriptor>,
    visiting: &mut BTreeSet<&'static str>,
    visited: &mut BTreeSet<&'static str>,
    ordered: &mut Vec<&'static ExtensionDescriptor>,
) -> Result<(), ExtensionRegistryError> {
    if visited.contains(name) {
        return Ok(());
    }
    if !visiting.insert(name) {
        return Err(ExtensionRegistryError::DependencyCycle);
    }
    let descriptor = modules[name];
    for dependency in descriptor.dependencies {
        if !modules.contains_key(dependency) {
            return Err(ExtensionRegistryError::MissingDependency {
                extension: name,
                dependency,
            });
        }
        visit(dependency, modules, visiting, visited, ordered)?;
    }
    visiting.remove(name);
    visited.insert(name);
    ordered.push(descriptor);
    Ok(())
}

static DEFAULT_REGISTRY: OnceLock<ExtensionRegistry> = OnceLock::new();

/// Full default registry used by CLI/server integration.
#[derive(Clone, Copy, Debug, Default)]
pub struct BuiltinRegistry;

impl BuiltinRegistry {
    pub const fn new() -> Self {
        Self
    }

    fn registry(self) -> &'static ExtensionRegistry {
        DEFAULT_REGISTRY.get_or_init(|| {
            ExtensionRegistry::assemble(&DEFAULT_MODULES).expect("default extensions are valid")
        })
    }

    pub fn entries(self) -> &'static [BuiltinEntry] {
        self.registry().entries()
    }

    pub fn get(self, name: &str) -> Option<BuiltinEntry> {
        self.registry().get(name)
    }

    pub fn contains(self, name: &str) -> bool {
        self.registry().contains(name)
    }

    /// Allocates the states selected by the integration registry for one request.
    pub fn create_request_state(self) -> RequestState {
        self.registry().create_request_state()
    }

    /// Returns a registration-time typed slot for an enabled extension.
    pub fn request_state_slot<T: 'static>(self, extension: &str) -> Option<ExtensionStateSlot<T>> {
        self.registry().request_state_slot(extension)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use php_runtime::api::{
        ApcuState, BuiltinCompatibility, BuiltinContext, BuiltinResult, RuntimeSourceSpan, Value,
    };

    fn noop(
        _context: &mut BuiltinContext<'_>,
        _args: Vec<Value>,
        _span: RuntimeSourceSpan,
    ) -> BuiltinResult {
        Ok(Value::Null)
    }

    const ENTRY: BuiltinEntry = BuiltinEntry::new(
        "contract_probe",
        noop,
        BuiltinCompatibility::InternalTestHelper,
    );

    struct TestModule(&'static ExtensionDescriptor);
    impl ExtensionModule for TestModule {
        fn descriptor(&self) -> &'static ExtensionDescriptor {
            self.0
        }
    }

    const EMPTY_STATE: Option<php_runtime::api::ExtensionStateFactory> = None;

    #[test]
    fn default_registry_contains_both_pilots_in_sorted_order() {
        let registry = BuiltinRegistry::new();
        assert!(registry.contains("ctype_digit"));
        assert!(registry.contains("apcu_store"));
        assert!(
            registry
                .entries()
                .windows(2)
                .all(|pair| pair[0].name() < pair[1].name())
        );
    }

    #[test]
    fn disabled_extensions_allocate_no_state() {
        let registry = ExtensionRegistry::assemble(&[]).expect("empty set");
        assert_eq!(registry.request_state_layout().slot_count(), 0);
        assert_eq!(registry.request_state_layout().payload_bytes(), 0);
        assert_eq!(registry.create_request_state().slot_count(), 0);
        assert!(registry.request_state_slot::<ApcuState>("apcu").is_none());
    }

    #[test]
    fn selected_extensions_receive_stable_typed_slots() {
        let registry = ExtensionRegistry::assemble(&DEFAULT_MODULES).expect("default registry");
        let slot = registry
            .request_state_slot::<ApcuState>("apcu")
            .expect("APCu slot");
        assert_eq!(slot.index(), 0);
        assert_eq!(registry.request_state_layout().slot_count(), 1);
        assert_eq!(
            registry.request_state_layout().payload_bytes(),
            std::mem::size_of::<ApcuState>()
        );
        assert!(registry.request_state_slot::<ApcuState>("ctype").is_none());

        let mut request = registry.create_request_state();
        request.get_mut(slot).expect("selected APCu state").store(
            b"registry-layout".to_vec(),
            Value::Int(1),
            0,
        );
        assert_eq!(
            request
                .get_mut(slot)
                .expect("same direct slot")
                .fetch(b"registry-layout"),
            Some(Value::Int(1))
        );
        request
            .get_mut(slot)
            .expect("same direct slot")
            .delete(b"registry-layout");
    }

    #[test]
    fn process_shared_extension_state_survives_request_owner_reset() {
        let registry = ExtensionRegistry::assemble(&[&APCU]).expect("APCu registry");
        let slot = registry
            .request_state_slot::<ApcuState>("apcu")
            .expect("APCu slot");
        let key = b"registry-process-shared".to_vec();
        let mut first = registry.create_request_state();
        first.get_mut(slot).expect("first APCu handle").delete(&key);
        first.get_mut(slot).expect("first APCu handle").store(
            key.clone(),
            Value::string("shared"),
            0,
        );
        drop(first);

        let mut second = registry.create_request_state();
        assert_eq!(
            second
                .get_mut(slot)
                .expect("second APCu handle")
                .fetch(&key),
            Some(Value::string("shared"))
        );
        second
            .get_mut(slot)
            .expect("second APCu handle")
            .delete(&key);
    }

    #[test]
    fn duplicate_functions_and_missing_dependencies_fail_closed() {
        static FIRST_DESC: ExtensionDescriptor = ExtensionDescriptor {
            name: "first",
            version: "1",
            dependencies: &[],
            functions: &[ENTRY],
            classes: &[],
            constants: &[],
            request_state: EMPTY_STATE,
            capabilities: &[],
            initialize: None,
            shutdown: None,
        };
        static SECOND_DESC: ExtensionDescriptor = ExtensionDescriptor {
            name: "second",
            version: "1",
            dependencies: &[],
            functions: &[ENTRY],
            classes: &[],
            constants: &[],
            request_state: EMPTY_STATE,
            capabilities: &[],
            initialize: None,
            shutdown: None,
        };
        static MISSING_DESC: ExtensionDescriptor = ExtensionDescriptor {
            name: "missing",
            version: "1",
            dependencies: &["absent"],
            functions: &[],
            classes: &[],
            constants: &[],
            request_state: EMPTY_STATE,
            capabilities: &[],
            initialize: None,
            shutdown: None,
        };
        static FIRST: TestModule = TestModule(&FIRST_DESC);
        static SECOND: TestModule = TestModule(&SECOND_DESC);
        static MISSING: TestModule = TestModule(&MISSING_DESC);
        assert_eq!(
            ExtensionRegistry::assemble(&[&FIRST, &SECOND]).unwrap_err(),
            ExtensionRegistryError::DuplicateFunction("contract_probe")
        );
        assert_eq!(
            ExtensionRegistry::assemble(&[&MISSING]).unwrap_err(),
            ExtensionRegistryError::MissingDependency {
                extension: "missing",
                dependency: "absent"
            }
        );
    }

    #[test]
    fn duplicate_extensions_and_cycles_fail_closed() {
        static FIRST_DESC: ExtensionDescriptor = ExtensionDescriptor {
            name: "same",
            version: "1",
            dependencies: &[],
            functions: &[],
            classes: &[],
            constants: &[],
            request_state: EMPTY_STATE,
            capabilities: &[],
            initialize: None,
            shutdown: None,
        };
        static SECOND_DESC: ExtensionDescriptor = ExtensionDescriptor {
            name: "same",
            version: "2",
            dependencies: &[],
            functions: &[],
            classes: &[],
            constants: &[],
            request_state: EMPTY_STATE,
            capabilities: &[],
            initialize: None,
            shutdown: None,
        };
        static CYCLE_A_DESC: ExtensionDescriptor = ExtensionDescriptor {
            name: "cycle-a",
            version: "1",
            dependencies: &["cycle-b"],
            functions: &[],
            classes: &[],
            constants: &[],
            request_state: EMPTY_STATE,
            capabilities: &[],
            initialize: None,
            shutdown: None,
        };
        static CYCLE_B_DESC: ExtensionDescriptor = ExtensionDescriptor {
            name: "cycle-b",
            version: "1",
            dependencies: &["cycle-a"],
            functions: &[],
            classes: &[],
            constants: &[],
            request_state: EMPTY_STATE,
            capabilities: &[],
            initialize: None,
            shutdown: None,
        };
        static FIRST: TestModule = TestModule(&FIRST_DESC);
        static SECOND: TestModule = TestModule(&SECOND_DESC);
        static CYCLE_A: TestModule = TestModule(&CYCLE_A_DESC);
        static CYCLE_B: TestModule = TestModule(&CYCLE_B_DESC);
        assert_eq!(
            ExtensionRegistry::assemble(&[&FIRST, &SECOND]).unwrap_err(),
            ExtensionRegistryError::DuplicateExtension("same")
        );
        assert_eq!(
            ExtensionRegistry::assemble(&[&CYCLE_A, &CYCLE_B]).unwrap_err(),
            ExtensionRegistryError::DependencyCycle
        );
    }
}

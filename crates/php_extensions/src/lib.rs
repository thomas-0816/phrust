//! Statically linked extension implementations and integration registry.

mod apcu;
mod ctype;

use php_runtime::api::{BuiltinEntry, BuiltinRegistry as RuntimeBuiltinRegistry};
use php_runtime::api::{ExtensionDescriptor, ExtensionModule};
use std::any::Any;
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
        Ok(Self {
            descriptors: ordered,
            entries,
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

    pub fn create_request_states(&self) -> Vec<(&'static str, Box<dyn Any>)> {
        self.descriptors
            .iter()
            .filter_map(|descriptor| {
                descriptor
                    .request_state
                    .map(|factory| (descriptor.name, (factory.create)()))
            })
            .collect()
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use php_runtime::api::{
        BuiltinCompatibility, BuiltinContext, BuiltinResult, RuntimeSourceSpan, Value,
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
        assert!(registry.create_request_states().is_empty());
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

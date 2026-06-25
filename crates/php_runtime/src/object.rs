//! Minimal object storage and class metadata for runtime.

use crate::Value;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_OBJECT_ID: AtomicU64 = AtomicU64::new(1);

/// Minimal runtime type adapter used by the VM for Semantic frontend annotations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuntimeType {
    /// `int`
    Int,
    /// `float`
    Float,
    /// `string`
    String,
    /// `array`
    Array,
    /// `callable`
    Callable,
    /// `iterable`
    Iterable,
    /// `object`
    Object,
    /// `bool`
    Bool,
    /// `null`
    Null,
    /// `void`
    Void,
    /// `mixed`
    Mixed,
    /// `never`
    Never,
    /// Literal `false`.
    False,
    /// Literal `true`.
    True,
    /// Class-like type.
    Class { name: String },
    /// Nullable simple type.
    Nullable { inner: Box<RuntimeType> },
    /// Union type; matches when any member matches.
    Union { members: Vec<RuntimeType> },
    /// Intersection type; matches when every member matches.
    Intersection { members: Vec<RuntimeType> },
    /// Disjunctive normal form; each clause is usually an intersection.
    Dnf { clauses: Vec<RuntimeType> },
}

/// Runtime class table entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassEntry {
    /// Canonical class lookup name.
    pub name: String,
    /// Canonical parent class lookup name, when declared.
    pub parent: Option<String>,
    /// Canonical interface names implemented or extended by this class-like.
    pub interfaces: Vec<String>,
    /// Runtime-visible instance methods.
    pub methods: Vec<ClassMethodEntry>,
    /// Runtime-visible instance properties.
    pub properties: Vec<ClassPropertyEntry>,
    /// Runtime-visible class constants.
    pub constants: Vec<ClassConstantEntry>,
    /// Runtime-visible enum cases.
    pub enum_cases: Vec<ClassEnumCaseEntry>,
    /// Runtime-visible attributes on this class-like declaration.
    pub attributes: Vec<AttributeEntry>,
    /// Backing type for backed enums.
    pub enum_backing_type: Option<ClassEnumBackingType>,
    /// Raw IR function ID for `__construct`, when present.
    pub constructor_id: Option<u32>,
    /// Class declaration flags.
    pub flags: ClassFlags,
}

/// Class declaration flags.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ClassFlags {
    /// Abstract class.
    pub is_abstract: bool,
    /// Final class.
    pub is_final: bool,
    /// Readonly class.
    pub is_readonly: bool,
    /// Interface metadata entry.
    pub is_interface: bool,
    /// Enum metadata entry.
    pub is_enum: bool,
}

/// Runtime method table entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassMethodEntry {
    /// Normalized method lookup name.
    pub name: String,
    /// Source class-like that contributed the method.
    pub origin_class: String,
    /// Raw IR function ID for the method body.
    pub function_id: u32,
    /// Method flags.
    pub flags: ClassMethodFlags,
    /// Runtime-visible attributes on this method declaration.
    pub attributes: Vec<AttributeEntry>,
}

/// Runtime method flags.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ClassMethodFlags {
    /// Static method.
    pub is_static: bool,
    /// Private method.
    pub is_private: bool,
    /// Protected method.
    pub is_protected: bool,
    /// Abstract method.
    pub is_abstract: bool,
    /// Final method.
    pub is_final: bool,
}

/// Runtime property table entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassPropertyEntry {
    /// Property name without `$`.
    pub name: String,
    /// Default value for new instances.
    pub default: Value,
    /// Optional runtime type enforced on property writes.
    pub type_: Option<RuntimeType>,
    /// Property flags.
    pub flags: ClassPropertyFlags,
    /// Property hook functions.
    pub hooks: ClassPropertyHooks,
    /// Runtime-visible attributes on this property declaration.
    pub attributes: Vec<AttributeEntry>,
}

/// Runtime property flags.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ClassPropertyFlags {
    /// Static property.
    pub is_static: bool,
    /// Private property.
    pub is_private: bool,
    /// Protected property.
    pub is_protected: bool,
    /// Private setter.
    pub set_is_private: bool,
    /// Protected setter.
    pub set_is_protected: bool,
    /// Readonly property.
    pub is_readonly: bool,
    /// Typed property.
    pub is_typed: bool,
}

/// Runtime property hook metadata.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ClassPropertyHooks {
    /// Raw IR function ID for `get`.
    pub get_function_id: Option<u32>,
    /// Raw IR function ID for `set`.
    pub set_function_id: Option<u32>,
    /// True when normal property storage is materialized.
    pub backed: bool,
}

/// Runtime class constant table entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassConstantEntry {
    /// Constant name without the class qualifier.
    pub name: String,
    /// Runtime value.
    pub value: Value,
    /// Constant flags.
    pub flags: ClassConstantFlags,
    /// Runtime-visible attributes on this class constant declaration.
    pub attributes: Vec<AttributeEntry>,
}

/// Runtime class constant flags.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ClassConstantFlags {
    /// Private constant.
    pub is_private: bool,
    /// Protected constant.
    pub is_protected: bool,
}

/// Runtime enum backing type.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClassEnumBackingType {
    /// `int` backed enum.
    Int,
    /// `string` backed enum.
    String,
}

/// Runtime enum case table entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassEnumCaseEntry {
    /// Case name without the class qualifier.
    pub name: String,
    /// Case backing value, when backed.
    pub value: Option<Value>,
    /// Runtime-visible attributes on this enum case declaration.
    pub attributes: Vec<AttributeEntry>,
}

/// Runtime/reflection-visible attribute metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttributeEntry {
    /// Source-spelled attribute name.
    pub name: String,
    /// Resolved canonical class name, when Semantic frontend resolved it.
    pub resolved_name: Option<String>,
    /// Runtime fallback class name, when PHP may resolve dynamically.
    pub fallback_name: Option<String>,
    /// Runtime argument values in source order.
    pub arguments: Vec<Value>,
    /// True when this attribute name appears repeatedly on the same target.
    pub repeated_on_target: bool,
    /// Source span encoded as `(file, start, end)`.
    pub span: Option<(u32, u32, u32)>,
}

#[derive(Debug)]
struct ObjectStorage {
    class_name: String,
    properties: HashMap<String, Value>,
    property_order: Vec<String>,
}

/// Reference to runtime object storage.
#[derive(Clone)]
pub struct ObjectRef {
    id: u64,
    storage: Rc<RefCell<ObjectStorage>>,
}

/// Weak debug handle to object storage for GC tests.
#[derive(Clone, Debug)]
pub struct WeakObjectHandle {
    id: u64,
    storage: Weak<RefCell<ObjectStorage>>,
}

impl WeakObjectHandle {
    /// Returns the stable object identity.
    #[must_use]
    pub const fn id(&self) -> u64 {
        self.id
    }

    /// Returns true when the object storage is still alive.
    #[must_use]
    pub fn is_alive(&self) -> bool {
        self.storage.strong_count() > 0
    }

    /// Upgrades this weak handle into an object reference when still alive.
    #[must_use]
    pub fn upgrade(&self) -> Option<ObjectRef> {
        self.storage.upgrade().map(|storage| ObjectRef {
            id: self.id,
            storage,
        })
    }
}

impl ObjectRef {
    /// Creates an object with properties initialized from the class entry.
    #[must_use]
    pub fn new(class: &ClassEntry) -> Self {
        let property_entries = class
            .properties
            .iter()
            .filter(|property| {
                !property.flags.is_static
                    && !((property.hooks.get_function_id.is_some()
                        || property.hooks.set_function_id.is_some())
                        && !property.hooks.backed)
            })
            .map(|property| (property.name.clone(), property.default.clone()))
            .collect::<Vec<_>>();
        let property_order = property_entries
            .iter()
            .map(|(name, _)| name.clone())
            .collect();
        let properties = property_entries.into_iter().collect();
        Self {
            id: NEXT_OBJECT_ID.fetch_add(1, Ordering::Relaxed),
            storage: Rc::new(RefCell::new(ObjectStorage {
                class_name: class.name.clone(),
                properties,
                property_order,
            })),
        }
    }

    /// Returns the stable object identity for tests and diagnostics.
    #[must_use]
    pub const fn id(&self) -> u64 {
        self.id
    }

    /// Returns the current `Rc` strong count for GC debug metadata.
    #[must_use]
    pub fn gc_refcount_estimate(&self) -> usize {
        Rc::strong_count(&self.storage)
    }

    /// Returns a weak debug handle for GC tests.
    #[must_use]
    pub fn weak_handle(&self) -> WeakObjectHandle {
        WeakObjectHandle {
            id: self.id,
            storage: Rc::downgrade(&self.storage),
        }
    }

    /// Returns the object's class name.
    #[must_use]
    pub fn class_name(&self) -> String {
        self.storage.borrow().class_name.clone()
    }

    /// Creates a new object identity with a shallow copy of the property map.
    #[must_use]
    pub fn clone_shallow(&self) -> Self {
        let storage = self.storage.borrow();
        Self {
            id: NEXT_OBJECT_ID.fetch_add(1, Ordering::Relaxed),
            storage: Rc::new(RefCell::new(ObjectStorage {
                class_name: storage.class_name.clone(),
                properties: storage.properties.clone(),
                property_order: storage.property_order.clone(),
            })),
        }
    }

    /// Reads a property value.
    #[must_use]
    pub fn get_property(&self, name: &str) -> Option<Value> {
        self.storage.borrow().properties.get(name).cloned()
    }

    /// Writes a property value.
    pub fn set_property(&self, name: impl Into<String>, value: Value) {
        let name = name.into();
        let mut storage = self.storage.borrow_mut();
        if !storage.properties.contains_key(&name) {
            storage.property_order.push(name.clone());
        }
        storage.properties.insert(name, value);
    }

    /// Removes a property value, returning whether it existed.
    pub fn unset_property(&self, name: &str) -> bool {
        let mut storage = self.storage.borrow_mut();
        let removed = storage.properties.remove(name).is_some();
        if removed {
            storage.property_order.retain(|entry| entry != name);
        }
        removed
    }

    /// Clears all stored properties as an internal GC action.
    ///
    /// This is not PHP-visible `unset()` semantics; it is only used by the
    /// runtime-semantics cycle-collection test hook after proving the object is not
    /// rooted.
    pub fn gc_clear_properties(&self) {
        let mut storage = self.storage.borrow_mut();
        storage.properties.clear();
        storage.property_order.clear();
    }

    /// Returns a snapshot of runtime properties in PHP insertion/declaration order.
    #[must_use]
    pub fn properties_snapshot(&self) -> Vec<(String, Value)> {
        let storage = self.storage.borrow();
        storage
            .property_order
            .iter()
            .filter_map(|name| {
                storage
                    .properties
                    .get(name)
                    .map(|value| (name.clone(), value.clone()))
            })
            .collect()
    }
}

impl fmt::Debug for ObjectRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ObjectRef")
            .field("id", &self.id)
            .field("class_name", &self.class_name())
            .finish()
    }
}

impl PartialEq for ObjectRef {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for ObjectRef {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_refs_preserve_identity_and_independent_properties() {
        let class = ClassEntry {
            name: "box".to_owned(),
            parent: None,
            interfaces: Vec::new(),
            methods: Vec::new(),
            properties: vec![ClassPropertyEntry {
                name: "value".to_owned(),
                default: Value::Null,
                type_: None,
                flags: ClassPropertyFlags::default(),
                hooks: ClassPropertyHooks::default(),
                attributes: Vec::new(),
            }],
            constants: Vec::new(),
            enum_cases: Vec::new(),
            attributes: Vec::new(),
            enum_backing_type: None,
            constructor_id: None,
            flags: ClassFlags::default(),
        };
        let one = ObjectRef::new(&class);
        let two = ObjectRef::new(&class);
        one.set_property("value", Value::Int(1));
        two.set_property("value", Value::Int(2));

        assert_ne!(one, two);
        assert_eq!(one.get_property("value"), Some(Value::Int(1)));
        assert_eq!(two.get_property("value"), Some(Value::Int(2)));
        assert_eq!(one.class_name(), "box");
    }

    #[test]
    fn object_clone_shallow_copies_properties_with_new_identity() {
        let class = ClassEntry {
            name: "box".to_owned(),
            parent: None,
            interfaces: Vec::new(),
            methods: Vec::new(),
            properties: vec![ClassPropertyEntry {
                name: "value".to_owned(),
                default: Value::Null,
                type_: None,
                flags: ClassPropertyFlags::default(),
                hooks: ClassPropertyHooks::default(),
                attributes: Vec::new(),
            }],
            constants: Vec::new(),
            enum_cases: Vec::new(),
            attributes: Vec::new(),
            enum_backing_type: None,
            constructor_id: None,
            flags: ClassFlags::default(),
        };
        let original = ObjectRef::new(&class);
        original.set_property("value", Value::Int(1));
        let copy = original.clone_shallow();

        assert_ne!(original, copy);
        assert_eq!(copy.class_name(), "box");
        assert_eq!(copy.get_property("value"), Some(Value::Int(1)));
        copy.set_property("value", Value::Int(2));
        assert_eq!(original.get_property("value"), Some(Value::Int(1)));
        assert_eq!(copy.get_property("value"), Some(Value::Int(2)));
    }

    #[test]
    fn destructor_queue_mvp_can_key_objects_by_stable_identity() {
        let class = ClassEntry {
            name: "destructible".to_owned(),
            parent: None,
            interfaces: Vec::new(),
            methods: Vec::new(),
            properties: Vec::new(),
            constants: Vec::new(),
            enum_cases: Vec::new(),
            attributes: Vec::new(),
            enum_backing_type: None,
            constructor_id: None,
            flags: ClassFlags::default(),
        };
        let original = ObjectRef::new(&class);
        let same_handle = original.clone();
        let shallow_copy = original.clone_shallow();

        assert_eq!(original.id(), same_handle.id());
        assert_ne!(original.id(), shallow_copy.id());
        assert_eq!(original.class_name(), "destructible");
        assert_eq!(shallow_copy.class_name(), "destructible");
    }

    #[test]
    fn object_refs_preserve_parent_metadata_and_declared_properties() {
        let class = ClassEntry {
            name: "child".to_owned(),
            parent: Some("base".to_owned()),
            interfaces: Vec::new(),
            methods: Vec::new(),
            properties: vec![
                ClassPropertyEntry {
                    name: "baseValue".to_owned(),
                    default: Value::Int(1),
                    type_: None,
                    flags: ClassPropertyFlags::default(),
                    hooks: ClassPropertyHooks::default(),
                    attributes: Vec::new(),
                },
                ClassPropertyEntry {
                    name: "childValue".to_owned(),
                    default: Value::Int(2),
                    type_: None,
                    flags: ClassPropertyFlags::default(),
                    hooks: ClassPropertyHooks::default(),
                    attributes: Vec::new(),
                },
            ],
            constants: Vec::new(),
            enum_cases: Vec::new(),
            attributes: Vec::new(),
            enum_backing_type: None,
            constructor_id: None,
            flags: ClassFlags::default(),
        };
        let object = ObjectRef::new(&class);

        assert_eq!(class.parent.as_deref(), Some("base"));
        assert_eq!(object.get_property("baseValue"), Some(Value::Int(1)));
        assert_eq!(object.get_property("childValue"), Some(Value::Int(2)));
    }

    #[test]
    fn property_storage_skips_static_slots_and_unsets_dynamic_slots() {
        let class = ClassEntry {
            name: "slots".to_owned(),
            parent: None,
            interfaces: Vec::new(),
            methods: Vec::new(),
            properties: vec![
                ClassPropertyEntry {
                    name: "instance".to_owned(),
                    default: Value::Int(1),
                    type_: None,
                    flags: ClassPropertyFlags::default(),
                    hooks: ClassPropertyHooks::default(),
                    attributes: Vec::new(),
                },
                ClassPropertyEntry {
                    name: "shared".to_owned(),
                    default: Value::Int(2),
                    type_: None,
                    flags: ClassPropertyFlags {
                        is_static: true,
                        ..ClassPropertyFlags::default()
                    },
                    hooks: ClassPropertyHooks::default(),
                    attributes: Vec::new(),
                },
            ],
            constants: Vec::new(),
            enum_cases: Vec::new(),
            attributes: Vec::new(),
            enum_backing_type: None,
            constructor_id: None,
            flags: ClassFlags::default(),
        };
        let object = ObjectRef::new(&class);

        assert_eq!(object.get_property("instance"), Some(Value::Int(1)));
        assert_eq!(object.get_property("shared"), None);

        object.set_property("dynamic", Value::Int(3));
        assert!(object.unset_property("dynamic"));
        assert_eq!(object.get_property("dynamic"), None);
        assert!(!object.unset_property("dynamic"));
    }

    #[test]
    fn property_hooks_virtual_properties_do_not_allocate_backing_storage() {
        let class = ClassEntry {
            name: "hooks".to_owned(),
            parent: None,
            interfaces: Vec::new(),
            methods: Vec::new(),
            properties: vec![ClassPropertyEntry {
                name: "virtualName".to_owned(),
                default: Value::Uninitialized,
                type_: Some(RuntimeType::String),
                flags: ClassPropertyFlags {
                    is_typed: true,
                    ..ClassPropertyFlags::default()
                },
                hooks: ClassPropertyHooks {
                    get_function_id: Some(1),
                    set_function_id: Some(2),
                    backed: false,
                },
                attributes: Vec::new(),
            }],
            constants: Vec::new(),
            enum_cases: Vec::new(),
            attributes: Vec::new(),
            enum_backing_type: None,
            constructor_id: None,
            flags: ClassFlags::default(),
        };

        let object = ObjectRef::new(&class);
        assert_eq!(object.get_property("virtualName"), None);
    }

    #[test]
    fn enum_case_metadata_initializes_name_and_value_slots() {
        let class = ClassEntry {
            name: "priority".to_owned(),
            parent: None,
            interfaces: vec!["unitenum".to_owned(), "backedenum".to_owned()],
            methods: Vec::new(),
            properties: vec![
                ClassPropertyEntry {
                    name: "name".to_owned(),
                    default: Value::Uninitialized,
                    type_: Some(RuntimeType::String),
                    flags: ClassPropertyFlags {
                        is_readonly: true,
                        is_typed: true,
                        ..ClassPropertyFlags::default()
                    },
                    hooks: ClassPropertyHooks::default(),
                    attributes: Vec::new(),
                },
                ClassPropertyEntry {
                    name: "value".to_owned(),
                    default: Value::Uninitialized,
                    type_: Some(RuntimeType::String),
                    flags: ClassPropertyFlags {
                        is_readonly: true,
                        is_typed: true,
                        ..ClassPropertyFlags::default()
                    },
                    hooks: ClassPropertyHooks::default(),
                    attributes: Vec::new(),
                },
            ],
            constants: Vec::new(),
            enum_cases: vec![ClassEnumCaseEntry {
                name: "High".to_owned(),
                value: Some(Value::String(crate::PhpString::from_test_str("high"))),
                attributes: Vec::new(),
            }],
            attributes: Vec::new(),
            enum_backing_type: Some(ClassEnumBackingType::String),
            constructor_id: None,
            flags: ClassFlags {
                is_final: true,
                is_enum: true,
                ..ClassFlags::default()
            },
        };

        let object = ObjectRef::new(&class);
        object.set_property(
            "name",
            Value::String(crate::PhpString::from_test_str("High")),
        );
        object.set_property(
            "value",
            Value::String(crate::PhpString::from_test_str("high")),
        );

        assert!(class.flags.is_enum);
        assert_eq!(class.enum_cases[0].name, "High");
        assert_eq!(
            object.get_property("name"),
            Some(Value::String(crate::PhpString::from_test_str("High")))
        );
        assert_eq!(
            object.get_property("value"),
            Some(Value::String(crate::PhpString::from_test_str("high")))
        );
    }

    #[test]
    fn attribute_metadata_preserves_names_arguments_repetition_and_span() {
        let attribute = AttributeEntry {
            name: "SourceName".to_owned(),
            resolved_name: Some("resolved\\sourcename".to_owned()),
            fallback_name: Some("fallback\\SourceName".to_owned()),
            arguments: vec![
                Value::String(crate::PhpString::from_test_str("arg")),
                Value::Int(7),
            ],
            repeated_on_target: true,
            span: Some((1, 20, 42)),
        };
        let class = ClassEntry {
            name: "with_attributes".to_owned(),
            parent: None,
            interfaces: Vec::new(),
            methods: Vec::new(),
            properties: Vec::new(),
            constants: Vec::new(),
            enum_cases: Vec::new(),
            attributes: vec![attribute],
            enum_backing_type: None,
            constructor_id: None,
            flags: ClassFlags::default(),
        };

        assert_eq!(class.attributes[0].name, "SourceName");
        assert_eq!(
            class.attributes[0].arguments,
            [
                Value::String(crate::PhpString::from_test_str("arg")),
                Value::Int(7)
            ]
        );
        assert!(class.attributes[0].repeated_on_target);
        assert_eq!(class.attributes[0].span, Some((1, 20, 42)));
    }

    #[test]
    fn reflection_metadata_preserves_class_members_flags_types_and_attributes() {
        let method_attribute = AttributeEntry {
            name: "Route".to_owned(),
            resolved_name: Some("app\\route".to_owned()),
            fallback_name: None,
            arguments: vec![Value::String(crate::PhpString::from_test_str("/items"))],
            repeated_on_target: false,
            span: Some((0, 12, 28)),
        };
        let class = ClassEntry {
            name: "reflectiontarget".to_owned(),
            parent: Some("basecontroller".to_owned()),
            interfaces: vec!["reflectioncontract".to_owned()],
            methods: vec![ClassMethodEntry {
                name: "show".to_owned(),
                origin_class: "ReflectionTarget".to_owned(),
                function_id: 11,
                flags: ClassMethodFlags {
                    is_static: true,
                    is_final: true,
                    ..ClassMethodFlags::default()
                },
                attributes: vec![method_attribute.clone()],
            }],
            properties: vec![ClassPropertyEntry {
                name: "id".to_owned(),
                default: Value::Int(7),
                type_: Some(RuntimeType::Int),
                flags: ClassPropertyFlags {
                    is_private: true,
                    is_typed: true,
                    ..ClassPropertyFlags::default()
                },
                hooks: ClassPropertyHooks::default(),
                attributes: Vec::new(),
            }],
            constants: vec![ClassConstantEntry {
                name: "LABEL".to_owned(),
                value: Value::String(crate::PhpString::from_test_str("items")),
                flags: ClassConstantFlags {
                    is_protected: true,
                    ..ClassConstantFlags::default()
                },
                attributes: Vec::new(),
            }],
            enum_cases: Vec::new(),
            attributes: Vec::new(),
            enum_backing_type: None,
            constructor_id: Some(3),
            flags: ClassFlags {
                is_final: true,
                ..ClassFlags::default()
            },
        };

        assert_eq!(class.parent.as_deref(), Some("basecontroller"));
        assert_eq!(class.interfaces, ["reflectioncontract"]);
        assert!(class.flags.is_final);
        assert_eq!(class.constructor_id, Some(3));
        assert_eq!(class.methods[0].origin_class, "ReflectionTarget");
        assert!(class.methods[0].flags.is_static);
        assert!(class.methods[0].flags.is_final);
        assert_eq!(class.methods[0].attributes, [method_attribute]);
        assert_eq!(class.properties[0].type_, Some(RuntimeType::Int));
        assert!(class.properties[0].flags.is_private);
        assert_eq!(class.properties[0].default, Value::Int(7));
        assert!(class.constants[0].flags.is_protected);
        assert_eq!(
            class.constants[0].value,
            Value::String(crate::PhpString::from_test_str("items"))
        );

        let enum_class = ClassEntry {
            name: "runtime_status_fixture".to_owned(),
            parent: None,
            interfaces: vec!["unitenum".to_owned(), "backedenum".to_owned()],
            methods: Vec::new(),
            properties: Vec::new(),
            constants: Vec::new(),
            enum_cases: vec![ClassEnumCaseEntry {
                name: "Ready".to_owned(),
                value: Some(Value::String(crate::PhpString::from_test_str("ready"))),
                attributes: Vec::new(),
            }],
            attributes: Vec::new(),
            enum_backing_type: Some(ClassEnumBackingType::String),
            constructor_id: None,
            flags: ClassFlags {
                is_enum: true,
                is_final: true,
                ..ClassFlags::default()
            },
        };
        assert!(enum_class.flags.is_enum);
        assert_eq!(
            enum_class.enum_backing_type,
            Some(ClassEnumBackingType::String)
        );
        assert_eq!(
            enum_class.enum_cases[0].value,
            Some(Value::String(crate::PhpString::from_test_str("ready")))
        );

        let closure = Value::closure(
            29,
            vec![crate::ClosureCaptureValue::by_value(
                "captured".to_owned(),
                Value::String(crate::PhpString::from_test_str("cap")),
            )],
        );
        match closure {
            Value::Callable(crate::CallableValue::Closure { function, captures }) => {
                assert_eq!(function, 29);
                assert_eq!(captures[0].name, "captured");
                assert_eq!(
                    captures[0].value(),
                    Some(&Value::String(crate::PhpString::from_test_str("cap")))
                );
            }
            other => panic!("expected closure callable, got {other:?}"),
        }
    }

    #[test]
    fn late_static_class_constants_remain_class_metadata() {
        let class = ClassEntry {
            name: "meta".to_owned(),
            parent: None,
            interfaces: Vec::new(),
            methods: Vec::new(),
            properties: Vec::new(),
            constants: vec![ClassConstantEntry {
                name: "LABEL".to_owned(),
                value: Value::String(crate::PhpString::from_test_str("meta")),
                flags: ClassConstantFlags::default(),
                attributes: Vec::new(),
            }],
            enum_cases: Vec::new(),
            attributes: Vec::new(),
            enum_backing_type: None,
            constructor_id: None,
            flags: ClassFlags::default(),
        };
        let object = ObjectRef::new(&class);

        assert_eq!(class.constants[0].name, "LABEL");
        assert_eq!(
            class.constants[0].value,
            Value::String(crate::PhpString::from_test_str("meta"))
        );
        assert_eq!(object.get_property("LABEL"), None);
    }

    #[test]
    fn trait_method_origin_metadata_is_not_lost() {
        let class = ClassEntry {
            name: "uses_trait".to_owned(),
            parent: None,
            interfaces: Vec::new(),
            methods: vec![ClassMethodEntry {
                name: "run".to_owned(),
                origin_class: "ReusableTrait".to_owned(),
                function_id: 7,
                flags: ClassMethodFlags::default(),
                attributes: Vec::new(),
            }],
            properties: Vec::new(),
            constants: Vec::new(),
            enum_cases: Vec::new(),
            attributes: Vec::new(),
            enum_backing_type: None,
            constructor_id: None,
            flags: ClassFlags::default(),
        };

        assert_eq!(class.methods[0].name, "run");
        assert_eq!(class.methods[0].origin_class, "ReusableTrait");
    }

    #[test]
    fn interface_metadata_is_preserved_on_class_entries() {
        let class = ClassEntry {
            name: "implementation".to_owned(),
            parent: Some("base".to_owned()),
            interfaces: vec!["runnable".to_owned(), "stringable".to_owned()],
            methods: Vec::new(),
            properties: Vec::new(),
            constants: Vec::new(),
            enum_cases: Vec::new(),
            attributes: Vec::new(),
            enum_backing_type: None,
            constructor_id: None,
            flags: ClassFlags::default(),
        };

        assert_eq!(class.interfaces, ["runnable", "stringable"]);
        assert!(!class.flags.is_interface);
    }

    #[test]
    fn iterator_interface_metadata_is_preserved() {
        let class = ClassEntry {
            name: "cursor".to_owned(),
            parent: None,
            interfaces: vec!["iterator".to_owned(), "iteratoraggregate".to_owned()],
            methods: Vec::new(),
            properties: Vec::new(),
            constants: Vec::new(),
            enum_cases: Vec::new(),
            attributes: Vec::new(),
            enum_backing_type: None,
            constructor_id: None,
            flags: ClassFlags::default(),
        };

        assert!(class.interfaces.iter().any(|name| name == "iterator"));
        assert!(
            class
                .interfaces
                .iter()
                .any(|name| name == "iteratoraggregate")
        );
    }

    #[test]
    fn magic_property_method_metadata_is_preserved_for_vm_dispatch() {
        let class = ClassEntry {
            name: "overloaded".to_owned(),
            parent: None,
            interfaces: Vec::new(),
            methods: vec![
                ClassMethodEntry {
                    name: "__get".to_owned(),
                    origin_class: "overloaded".to_owned(),
                    function_id: 10,
                    flags: ClassMethodFlags::default(),
                    attributes: Vec::new(),
                },
                ClassMethodEntry {
                    name: "__set".to_owned(),
                    origin_class: "overloaded".to_owned(),
                    function_id: 11,
                    flags: ClassMethodFlags::default(),
                    attributes: Vec::new(),
                },
                ClassMethodEntry {
                    name: "__isset".to_owned(),
                    origin_class: "overloaded".to_owned(),
                    function_id: 12,
                    flags: ClassMethodFlags::default(),
                    attributes: Vec::new(),
                },
                ClassMethodEntry {
                    name: "__unset".to_owned(),
                    origin_class: "overloaded".to_owned(),
                    function_id: 13,
                    flags: ClassMethodFlags::default(),
                    attributes: Vec::new(),
                },
            ],
            properties: Vec::new(),
            constants: Vec::new(),
            enum_cases: Vec::new(),
            attributes: Vec::new(),
            enum_backing_type: None,
            constructor_id: None,
            flags: ClassFlags::default(),
        };
        let object = ObjectRef::new(&class);

        assert_eq!(object.class_name(), "overloaded");
        assert!(object.id() > 0);
        assert_eq!(
            class
                .methods
                .iter()
                .map(|method| method.name.as_str())
                .collect::<Vec<_>>(),
            ["__get", "__set", "__isset", "__unset"]
        );
    }

    #[test]
    fn serialization_magic_method_metadata_is_preserved_for_gap_reporting() {
        let class = ClassEntry {
            name: "serializable_box".to_owned(),
            parent: None,
            interfaces: Vec::new(),
            methods: vec![
                ClassMethodEntry {
                    name: "__serialize".to_owned(),
                    origin_class: "serializable_box".to_owned(),
                    function_id: 20,
                    flags: ClassMethodFlags::default(),
                    attributes: Vec::new(),
                },
                ClassMethodEntry {
                    name: "__unserialize".to_owned(),
                    origin_class: "serializable_box".to_owned(),
                    function_id: 21,
                    flags: ClassMethodFlags::default(),
                    attributes: Vec::new(),
                },
                ClassMethodEntry {
                    name: "__sleep".to_owned(),
                    origin_class: "serializable_box".to_owned(),
                    function_id: 22,
                    flags: ClassMethodFlags::default(),
                    attributes: Vec::new(),
                },
                ClassMethodEntry {
                    name: "__wakeup".to_owned(),
                    origin_class: "serializable_box".to_owned(),
                    function_id: 23,
                    flags: ClassMethodFlags::default(),
                    attributes: Vec::new(),
                },
            ],
            properties: Vec::new(),
            constants: Vec::new(),
            enum_cases: Vec::new(),
            attributes: Vec::new(),
            enum_backing_type: None,
            constructor_id: None,
            flags: ClassFlags::default(),
        };

        assert_eq!(
            class
                .methods
                .iter()
                .map(|method| method.name.as_str())
                .collect::<Vec<_>>(),
            ["__serialize", "__unserialize", "__sleep", "__wakeup"]
        );
    }
}

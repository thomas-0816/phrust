use super::*;
use crate::Value;

mod identity_storage {
    use super::*;

    #[test]
    fn object_refs_preserve_identity_and_independent_properties() {
        let class = ClassEntry {
            name: "box".to_owned().into(),
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
        assert_eq!(
            one.try_get_property("value").expect("checked read"),
            Some(Value::Int(1))
        );
        one.try_set_property("extra", Value::Bool(true))
            .expect("checked write");
        assert_eq!(
            one.try_properties_snapshot().expect("checked snapshot"),
            vec![
                ("value".to_owned(), Value::Int(1)),
                ("extra".to_owned(), Value::Bool(true))
            ]
        );
        assert!(
            one.try_any_property_value(|value| value == &Value::Bool(true))
                .expect("checked property visit")
        );
        assert!(
            !one.try_any_property_value(|value| value == &Value::Int(99))
                .expect("checked property visit")
        );
        assert_eq!(one.class_name(), "box");
    }

    #[test]
    fn object_clone_shallow_copies_properties_with_new_identity() {
        let class = ClassEntry {
            name: "box".to_owned().into(),
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
            name: "destructible".to_owned().into(),
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
    fn released_object_ids_are_reused_in_lifo_order() {
        let class = ClassEntry {
            name: "box".to_owned().into(),
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
        let first = ObjectRef::new(&class);
        let second = ObjectRef::new(&class);
        let first_id = first.id();
        let second_id = second.id();

        first.release_php_handle();
        second.release_php_handle();

        let reused_first = ObjectRef::new(&class);
        let reused_second = ObjectRef::new(&class);

        assert_eq!(reused_first.id(), second_id);
        assert_eq!(reused_second.id(), first_id);
    }

    #[test]
    fn object_refs_preserve_parent_metadata_and_declared_properties() {
        let class = ClassEntry {
            name: "child".to_owned().into(),
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
    fn object_storage_keeps_first_order_slot_for_overridden_properties() {
        let class = ClassEntry {
            name: "child".to_owned().into(),
            parent: Some("base".to_owned()),
            interfaces: Vec::new(),
            methods: Vec::new(),
            properties: vec![
                ClassPropertyEntry {
                    name: "value".to_owned(),
                    default: Value::String(crate::PhpString::from_test_str("base")),
                    type_: None,
                    flags: ClassPropertyFlags::default(),
                    hooks: ClassPropertyHooks::default(),
                    attributes: Vec::new(),
                },
                ClassPropertyEntry {
                    name: "other".to_owned(),
                    default: Value::Int(1),
                    type_: None,
                    flags: ClassPropertyFlags::default(),
                    hooks: ClassPropertyHooks::default(),
                    attributes: Vec::new(),
                },
                ClassPropertyEntry {
                    name: "private:base:hidden".to_owned(),
                    default: Value::Int(2),
                    type_: None,
                    flags: ClassPropertyFlags {
                        is_private: true,
                        ..ClassPropertyFlags::default()
                    },
                    hooks: ClassPropertyHooks::default(),
                    attributes: Vec::new(),
                },
                ClassPropertyEntry {
                    name: "value".to_owned(),
                    default: Value::String(crate::PhpString::from_test_str("child")),
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
        let properties = object.properties_snapshot();

        assert_eq!(properties.len(), 3);
        assert_eq!(properties[0].0, "value");
        assert_eq!(
            properties[0].1,
            Value::String(crate::PhpString::from_test_str("child"))
        );
        assert_eq!(properties[1], ("other".to_owned(), Value::Int(1)));
        assert_eq!(
            properties[2],
            ("private:base:hidden".to_owned(), Value::Int(2))
        );
    }

    #[test]
    fn property_storage_skips_static_slots_and_unsets_dynamic_slots() {
        let class = ClassEntry {
            name: "slots".to_owned().into(),
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
    fn private_property_debug_label_uses_encoded_declaring_class() {
        let class = ClassEntry {
            name: "child".to_owned().into(),
            parent: Some("base".to_owned()),
            interfaces: Vec::new(),
            methods: Vec::new(),
            properties: vec![ClassPropertyEntry {
                name: "private:base:member".to_owned(),
                default: Value::String(crate::PhpString::from_test_str("value")),
                type_: None,
                flags: ClassPropertyFlags {
                    is_private: true,
                    ..ClassPropertyFlags::default()
                },
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
        let object = ObjectRef::new_with_display_name(&class, "child");

        assert_eq!(
            object.property_debug_label("private:base:member"),
            "\"member\":\"base\":private"
        );
    }

    #[test]
    fn property_hooks_virtual_properties_do_not_allocate_backing_storage() {
        let class = ClassEntry {
            name: "hooks".to_owned().into(),
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

    /// A class exercising every backing/skip case the slot builder must
    /// reproduce: an inherited-then-overridden duplicate name (last default
    /// wins at the first-occurrence slot), a static (excluded), a virtual
    /// hooked property (unbacked, excluded), a backed hooked property, a
    /// readonly typed property with a default, and a typed-uninitialized
    /// property (present with the `Uninitialized` sentinel, not absent).
    fn representative_class() -> ClassEntry {
        ClassEntry {
            name: "child".to_owned().into(),
            parent: Some("base".to_owned()),
            interfaces: Vec::new(),
            methods: Vec::new(),
            properties: vec![
                ClassPropertyEntry {
                    name: "value".to_owned(),
                    default: Value::String(crate::PhpString::from_test_str("base")),
                    type_: None,
                    flags: ClassPropertyFlags::default(),
                    hooks: ClassPropertyHooks::default(),
                    attributes: Vec::new(),
                },
                ClassPropertyEntry {
                    name: "sharedStatic".to_owned(),
                    default: Value::Int(99),
                    type_: None,
                    flags: ClassPropertyFlags {
                        is_static: true,
                        ..ClassPropertyFlags::default()
                    },
                    hooks: ClassPropertyHooks::default(),
                    attributes: Vec::new(),
                },
                ClassPropertyEntry {
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
                },
                ClassPropertyEntry {
                    name: "backedHook".to_owned(),
                    default: Value::Int(7),
                    type_: Some(RuntimeType::Int),
                    flags: ClassPropertyFlags {
                        is_typed: true,
                        ..ClassPropertyFlags::default()
                    },
                    hooks: ClassPropertyHooks {
                        get_function_id: Some(3),
                        set_function_id: None,
                        backed: true,
                    },
                    attributes: Vec::new(),
                },
                ClassPropertyEntry {
                    name: "readonlyLimit".to_owned(),
                    default: Value::Int(42),
                    type_: Some(RuntimeType::Int),
                    flags: ClassPropertyFlags {
                        is_readonly: true,
                        is_typed: true,
                        ..ClassPropertyFlags::default()
                    },
                    hooks: ClassPropertyHooks::default(),
                    attributes: Vec::new(),
                },
                ClassPropertyEntry {
                    name: "uninitialized".to_owned(),
                    default: Value::Uninitialized,
                    type_: Some(RuntimeType::Int),
                    flags: ClassPropertyFlags {
                        is_typed: true,
                        ..ClassPropertyFlags::default()
                    },
                    hooks: ClassPropertyHooks::default(),
                    attributes: Vec::new(),
                },
                ClassPropertyEntry {
                    name: "private:base:hidden".to_owned(),
                    default: Value::Int(2),
                    type_: None,
                    flags: ClassPropertyFlags {
                        is_private: true,
                        ..ClassPropertyFlags::default()
                    },
                    hooks: ClassPropertyHooks::default(),
                    attributes: Vec::new(),
                },
                // Overriding declaration of `value`: keeps the first-occurrence
                // slot but the later default must win.
                ClassPropertyEntry {
                    name: "value".to_owned(),
                    default: Value::String(crate::PhpString::from_test_str("child")),
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
        }
    }

    #[test]
    fn from_layout_slots_reproduces_new_with_display_name_initial_state() {
        let class = representative_class();
        let names = [
            "value",
            "sharedStatic",
            "virtualName",
            "backedHook",
            "readonlyLimit",
            "uninitialized",
            "private:base:hidden",
        ];

        let slow = ObjectRef::new_with_display_name(&class, "child");
        let template = ObjectRef::default_declared_slots(&class, "child");
        let fast = ObjectRef::from_layout_slots(&class, "child", template.clone());

        // PHP-visible snapshot (declaration/slot order + values) is identical.
        assert_eq!(slow.properties_snapshot(), fast.properties_snapshot());

        // The overriding declaration's default wins at the first-occurrence slot.
        assert_eq!(
            fast.get_property("value"),
            Some(Value::String(crate::PhpString::from_test_str("child")))
        );
        // Static is excluded from instance storage.
        assert_eq!(fast.get_property("sharedStatic"), None);
        // Virtual (unbacked) hook has no backing slot.
        assert_eq!(fast.get_property("virtualName"), None);
        // Backed hook keeps its default.
        assert_eq!(fast.get_property("backedHook"), Some(Value::Int(7)));

        // Per-name reads and debug labels match the slow path exactly.
        for name in names {
            assert_eq!(
                slow.get_property(name),
                fast.get_property(name),
                "property `{name}` read differs between construction paths"
            );
            assert_eq!(
                slow.property_debug_label(name),
                fast.property_debug_label(name),
                "property `{name}` debug label differs between construction paths"
            );
        }

        // The typed-uninitialized backed slot is present-but-uninitialized (the
        // `Uninitialized` sentinel occupies the slot; that is distinct from an
        // absent `None` slot). The template must carry the sentinel identically.
        assert_eq!(
            fast.get_property("uninitialized"),
            Some(Value::Uninitialized)
        );
        assert!(
            fast.properties_snapshot()
                .iter()
                .any(|(n, v)| n == "uninitialized" && *v == Value::Uninitialized),
            "uninitialized typed slot must surface with the Uninitialized sentinel"
        );

        // The template length matches the layout slot count (backed instance
        // names, deduped): value, backedHook, readonlyLimit, uninitialized,
        // private:base:hidden.
        assert_eq!(template.len(), 5);
    }

    #[test]
    fn default_declared_slots_template_is_display_name_independent() {
        let class = representative_class();
        // Different display spellings select different debug-label layout
        // variants but must produce the same slot template contents.
        assert_eq!(
            ObjectRef::default_declared_slots(&class, "Child"),
            ObjectRef::default_declared_slots(&class, "child"),
        );
    }
}

mod enum_metadata {
    use super::*;

    #[test]
    fn enum_case_metadata_initializes_name_and_value_slots() {
        let class = ClassEntry {
            name: "priority".to_owned().into(),
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
}

mod attribute_reflection_metadata {
    use super::*;

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
            name: "with_attributes".to_owned().into(),
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
            name: "reflectiontarget".to_owned().into(),
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
            name: "runtime_status_fixture".to_owned().into(),
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

        let closure = Value::closure(crate::ClosurePayload::new(
            29,
            vec![crate::ClosureCaptureValue::by_value(
                "captured".to_owned(),
                Value::String(crate::PhpString::from_test_str("cap")),
            )],
        ));
        let payload = closure
            .as_closure()
            .unwrap_or_else(|| panic!("expected closure callable, got {closure:?}"));
        assert_eq!(payload.function, 29);
        assert_eq!(payload.captures[0].name, "captured");
        assert_eq!(
            payload.captures[0].value(),
            Some(&Value::String(crate::PhpString::from_test_str("cap")))
        );
    }
}

mod class_metadata {
    use super::*;

    #[test]
    fn late_static_class_constants_remain_class_metadata() {
        let class = ClassEntry {
            name: "meta".to_owned().into(),
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
            name: "uses_trait".to_owned().into(),
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
            name: "implementation".to_owned().into(),
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
            name: "cursor".to_owned().into(),
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
}

mod magic_metadata {
    use super::*;

    #[test]
    fn magic_property_method_metadata_is_preserved_for_vm_dispatch() {
        let class = ClassEntry {
            name: "overloaded".to_owned().into(),
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
            name: "serializable_box".to_owned().into(),
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

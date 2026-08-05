//! Fixed internal class publication plans.

use super::*;

impl NativeRequestFastState {
    /// Returns the canonical layouts for internal classes whose objects can
    /// enter generated code. Cold metadata publication copies these records
    /// into immutable `instanceof` tables; exact warm leaves continue to use
    /// their cached prepared-class pointers directly.
    pub(crate) fn prepared_internal_class_layouts(&self) -> Vec<(String, u64)> {
        self.internal_class_plans
            .iter()
            .map(|(name, (prepared, _))| (name.clone(), prepared.layout_id))
            .collect()
    }

    pub(crate) fn prepare_constructorless_stdclass_plan(&mut self) {
        if self.internal_class_plans.contains_key("stdclass") {
            return;
        }
        let entry = php_runtime::api::ClassEntry {
            name: std::sync::Arc::from("stdclass"),
            parent: None,
            interfaces: Vec::new(),
            methods: Vec::new(),
            properties: Vec::new(),
            constants: Vec::new(),
            enum_cases: Vec::new(),
            attributes: Vec::new(),
            enum_backing_type: None,
            constructor_id: None,
            flags: php_runtime::api::ClassFlags {
                has_complete_method_table: true,
                ..php_runtime::api::ClassFlags::default()
            },
        };
        let display_name = "stdClass".to_owned();
        let layout_id = php_runtime::api::ObjectRef::prepared_layout_id(&entry, &display_name);
        let prepared = Box::new(PreparedNativeRuntimeClass {
            entry,
            display_name,
            layout_id,
            default_native_slots: Box::new([]),
        });
        let plan = Box::new(php_jit::JitNativePreparedClassPlan {
            prepared: std::ptr::from_ref(prepared.as_ref()) as usize as u64,
            display_name_bytes: prepared.display_name.as_ptr() as usize as u64,
            display_name_length: prepared.display_name.len() as u64,
            state: php_jit::JIT_NATIVE_PREPARED_CLASS_ALLOCATABLE,
            flags: 0,
        });
        self.internal_class_plans
            .insert("stdclass".to_owned(), (prepared, plan));
    }

    /// Publishes the fixed native `DateTime` allocation layout used by the
    /// exact procedural constructor. The separate pointer is intentionally
    /// cached here so the warm leaf never re-enters the internal-class map.
    pub(crate) fn prepare_datetime_class_plan(&mut self) {
        if let Some((prepared, _)) = self.internal_class_plans.get("datetime") {
            self.prepared_datetime_class = std::ptr::from_ref(prepared.as_ref());
            return;
        }
        let entry = php_runtime::api::ClassEntry {
            name: std::sync::Arc::from("datetime"),
            parent: None,
            interfaces: Vec::new(),
            methods: Vec::new(),
            properties: Vec::new(),
            constants: Vec::new(),
            enum_cases: Vec::new(),
            attributes: Vec::new(),
            enum_backing_type: None,
            constructor_id: None,
            flags: php_runtime::api::ClassFlags::default(),
        };
        let display_name = "DateTime".to_owned();
        let layout_id = php_runtime::api::ObjectRef::prepared_layout_id(&entry, &display_name);
        let prepared = Box::new(PreparedNativeRuntimeClass {
            entry,
            display_name,
            layout_id,
            default_native_slots: Box::new([]),
        });
        self.prepared_datetime_class = std::ptr::from_ref(prepared.as_ref());
        let plan = Box::new(php_jit::JitNativePreparedClassPlan {
            prepared: self.prepared_datetime_class as usize as u64,
            display_name_bytes: prepared.display_name.as_ptr() as usize as u64,
            display_name_length: prepared.display_name.len() as u64,
            state: php_jit::JIT_NATIVE_PREPARED_CLASS_ALLOCATABLE,
            flags: php_jit::JIT_NATIVE_PREPARED_CLASS_HAS_CONSTRUCTOR,
        });
        self.internal_class_plans
            .insert("datetime".to_owned(), (prepared, plan));
    }

    /// Publishes the fixed native `DateTimeZone` allocation layout used by
    /// exact `timezone_open()`.
    pub(crate) fn prepare_datetimezone_class_plan(&mut self) {
        if let Some((prepared, _)) = self.internal_class_plans.get("datetimezone") {
            self.prepared_datetimezone_class = std::ptr::from_ref(prepared.as_ref());
            return;
        }
        let entry = php_runtime::api::ClassEntry {
            name: std::sync::Arc::from("datetimezone"),
            parent: None,
            interfaces: Vec::new(),
            methods: Vec::new(),
            properties: Vec::new(),
            constants: Vec::new(),
            enum_cases: Vec::new(),
            attributes: Vec::new(),
            enum_backing_type: None,
            constructor_id: None,
            flags: php_runtime::api::ClassFlags::default(),
        };
        let display_name = "DateTimeZone".to_owned();
        let layout_id = php_runtime::api::ObjectRef::prepared_layout_id(&entry, &display_name);
        let prepared = Box::new(PreparedNativeRuntimeClass {
            entry,
            display_name,
            layout_id,
            default_native_slots: Box::new([]),
        });
        self.prepared_datetimezone_class = std::ptr::from_ref(prepared.as_ref());
        let plan = Box::new(php_jit::JitNativePreparedClassPlan {
            prepared: self.prepared_datetimezone_class as usize as u64,
            display_name_bytes: prepared.display_name.as_ptr() as usize as u64,
            display_name_length: prepared.display_name.len() as u64,
            state: php_jit::JIT_NATIVE_PREPARED_CLASS_ALLOCATABLE,
            flags: php_jit::JIT_NATIVE_PREPARED_CLASS_HAS_CONSTRUCTOR,
        });
        self.internal_class_plans
            .insert("datetimezone".to_owned(), (prepared, plan));
    }

    /// Publishes the immutable native `finfo` object allocation plan.
    pub(crate) fn prepare_finfo_class_plan(&mut self) {
        if let Some((prepared, _)) = self.internal_class_plans.get("finfo") {
            self.prepared_finfo_class = std::ptr::from_ref(prepared.as_ref());
            return;
        }
        let entry = php_runtime::api::ClassEntry {
            name: std::sync::Arc::from("finfo"),
            parent: None,
            interfaces: Vec::new(),
            methods: Vec::new(),
            properties: Vec::new(),
            constants: Vec::new(),
            enum_cases: Vec::new(),
            attributes: Vec::new(),
            enum_backing_type: None,
            constructor_id: None,
            flags: php_runtime::api::ClassFlags {
                is_final: true,
                ..php_runtime::api::ClassFlags::default()
            },
        };
        let display_name = "finfo".to_owned();
        let layout_id = php_runtime::api::ObjectRef::prepared_layout_id(&entry, &display_name);
        let prepared = Box::new(PreparedNativeRuntimeClass {
            entry,
            display_name,
            layout_id,
            default_native_slots: Box::new([]),
        });
        self.prepared_finfo_class = std::ptr::from_ref(prepared.as_ref());
        let plan = Box::new(php_jit::JitNativePreparedClassPlan {
            prepared: self.prepared_finfo_class as usize as u64,
            display_name_bytes: prepared.display_name.as_ptr() as usize as u64,
            display_name_length: prepared.display_name.len() as u64,
            state: php_jit::JIT_NATIVE_PREPARED_CLASS_ALLOCATABLE,
            flags: 0,
        });
        self.internal_class_plans
            .insert("finfo".to_owned(), (prepared, plan));
    }

    /// Publishes the fixed native `mysqli_result` layout returned by exact queries.
    pub(crate) fn prepare_mysqli_class_plan(&mut self) {
        if let Some((prepared, _)) = self.internal_class_plans.get("mysqli") {
            self.prepared_mysqli_class = std::ptr::from_ref(prepared.as_ref());
            return;
        }
        let entry = php_runtime::api::ClassEntry {
            name: std::sync::Arc::from("mysqli"),
            parent: None,
            interfaces: Vec::new(),
            methods: Vec::new(),
            properties: Vec::new(),
            constants: Vec::new(),
            enum_cases: Vec::new(),
            attributes: Vec::new(),
            enum_backing_type: None,
            constructor_id: None,
            flags: php_runtime::api::ClassFlags::default(),
        };
        let display_name = "mysqli".to_owned();
        let layout_id = php_runtime::api::ObjectRef::prepared_layout_id(&entry, &display_name);
        let prepared = Box::new(PreparedNativeRuntimeClass {
            entry,
            display_name,
            layout_id,
            default_native_slots: Box::new([]),
        });
        self.prepared_mysqli_class = std::ptr::from_ref(prepared.as_ref());
        let plan = Box::new(php_jit::JitNativePreparedClassPlan {
            prepared: self.prepared_mysqli_class as usize as u64,
            display_name_bytes: prepared.display_name.as_ptr() as usize as u64,
            display_name_length: prepared.display_name.len() as u64,
            state: php_jit::JIT_NATIVE_PREPARED_CLASS_ALLOCATABLE,
            flags: 0,
        });
        self.internal_class_plans
            .insert("mysqli".to_owned(), (prepared, plan));
    }

    /// Publishes the fixed native `mysqli_result` layout returned by exact queries.
    pub(crate) fn prepare_mysqli_result_class_plan(&mut self) {
        if let Some((prepared, _)) = self.internal_class_plans.get("mysqli_result") {
            self.prepared_mysqli_result_class = std::ptr::from_ref(prepared.as_ref());
            return;
        }
        let entry = php_runtime::api::ClassEntry {
            name: std::sync::Arc::from("mysqli_result"),
            parent: None,
            interfaces: Vec::new(),
            methods: Vec::new(),
            properties: Vec::new(),
            constants: Vec::new(),
            enum_cases: Vec::new(),
            attributes: Vec::new(),
            enum_backing_type: None,
            constructor_id: None,
            flags: php_runtime::api::ClassFlags::default(),
        };
        let display_name = "mysqli_result".to_owned();
        let layout_id = php_runtime::api::ObjectRef::prepared_layout_id(&entry, &display_name);
        let prepared = Box::new(PreparedNativeRuntimeClass {
            entry,
            display_name,
            layout_id,
            default_native_slots: Box::new([]),
        });
        self.prepared_mysqli_result_class = std::ptr::from_ref(prepared.as_ref());
        let plan = Box::new(php_jit::JitNativePreparedClassPlan {
            prepared: self.prepared_mysqli_result_class as usize as u64,
            display_name_bytes: prepared.display_name.as_ptr() as usize as u64,
            display_name_length: prepared.display_name.len() as u64,
            state: php_jit::JIT_NATIVE_PREPARED_CLASS_ALLOCATABLE,
            flags: 0,
        });
        self.internal_class_plans
            .insert("mysqli_result".to_owned(), (prepared, plan));
    }

    pub(super) fn internal_class_plan(&self, class: &str) -> Option<u64> {
        self.internal_class_plans
            .get(class)
            .map(|(_, plan)| std::ptr::from_ref(plan.as_ref()) as usize as u64)
    }
}

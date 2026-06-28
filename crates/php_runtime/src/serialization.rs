//! Bounded PHP serialization MVP for standard-library.

use crate::{
    ArrayKey, ClassEntry, ClassFlags, ObjectRef, PhpArray, PhpString, Value, display_class_name,
    normalize_class_name,
};

const DEFAULT_MAX_DEPTH: usize = 64;
const DEFAULT_MAX_ITEMS: usize = 16_384;
const DEFAULT_MAX_BYTES: usize = 1_048_576;

/// Security and compatibility limits for `unserialize`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnserializeOptions {
    /// Maximum recursive container depth.
    pub max_depth: usize,
    /// Maximum total parsed array/object entries.
    pub max_items: usize,
    /// Maximum accepted input byte length.
    pub max_bytes: usize,
}

impl Default for UnserializeOptions {
    fn default() -> Self {
        Self {
            max_depth: DEFAULT_MAX_DEPTH,
            max_items: DEFAULT_MAX_ITEMS,
            max_bytes: DEFAULT_MAX_BYTES,
        }
    }
}

/// Stable serialization error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SerializationError {
    message: String,
}

impl SerializationError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// Human-readable message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Serializes one runtime value in PHP's wire format for the standard-library MVP.
pub fn serialize(value: &Value) -> Result<PhpString, SerializationError> {
    let mut writer = Serializer::default();
    writer.write_value(value, 0)?;
    Ok(PhpString::from_bytes(writer.output))
}

/// Parses one PHP serialized value with bounded recursion and allocation.
pub fn unserialize(
    input: &PhpString,
    options: UnserializeOptions,
) -> Result<Value, SerializationError> {
    if input.len() > options.max_bytes {
        return Err(SerializationError::new(
            "serialized input exceeds byte limit",
        ));
    }
    let mut parser = Parser {
        bytes: input.as_bytes(),
        offset: 0,
        options,
        parsed_items: 0,
    };
    let value = parser.parse_value(0)?;
    if parser.offset != parser.bytes.len() {
        return Err(SerializationError::new(
            "trailing bytes after serialized value",
        ));
    }
    Ok(value)
}

#[derive(Default)]
struct Serializer {
    output: Vec<u8>,
    active_references: Vec<usize>,
}

impl Serializer {
    fn write_value(&mut self, value: &Value, depth: usize) -> Result<(), SerializationError> {
        if depth > DEFAULT_MAX_DEPTH {
            return Err(SerializationError::new(
                "serialization depth limit exceeded",
            ));
        }
        match value {
            Value::Null | Value::Uninitialized => self.output.extend_from_slice(b"N;"),
            Value::Bool(false) => self.output.extend_from_slice(b"b:0;"),
            Value::Bool(true) => self.output.extend_from_slice(b"b:1;"),
            Value::Int(value) => self
                .output
                .extend_from_slice(format!("i:{value};").as_bytes()),
            Value::Float(value) => self
                .output
                .extend_from_slice(format!("d:{};", serialize_float(*value)).as_bytes()),
            Value::String(value) => {
                self.output
                    .extend_from_slice(format!("s:{}:\"", value.len()).as_bytes());
                self.output.extend_from_slice(value.as_bytes());
                self.output.extend_from_slice(b"\";");
            }
            Value::Array(array) => {
                self.output
                    .extend_from_slice(format!("a:{}:{{", array.len()).as_bytes());
                for (key, element) in array.iter() {
                    self.write_key(key);
                    self.write_value(element, depth + 1)?;
                }
                self.output.extend_from_slice(b"}");
            }
            Value::Object(object) => {
                let class = object.display_name();
                let properties = object.properties_snapshot();
                self.output
                    .extend_from_slice(format!("O:{}:\"", class.len()).as_bytes());
                self.output.extend_from_slice(class.as_bytes());
                self.output
                    .extend_from_slice(format!("\":{}:{{", properties.len()).as_bytes());
                for (name, property) in properties {
                    self.write_value(
                        &Value::string(serialized_object_property_name(object, &name)),
                        depth + 1,
                    )?;
                    self.write_value(&property, depth + 1)?;
                }
                self.output.extend_from_slice(b"}");
            }
            Value::Fiber(_) | Value::Generator(_) | Value::Callable(_) => {
                return Err(SerializationError::new(
                    "serialization for this object-like runtime value is not implemented",
                ));
            }
            Value::Resource(_) => {
                return Err(SerializationError::new(
                    "serialization for resources is not implemented",
                ));
            }
            Value::Reference(cell) => {
                let id = cell.gc_debug_id();
                if self.active_references.contains(&id) {
                    self.output.extend_from_slice(b"N;");
                    return Ok(());
                }
                self.active_references.push(id);
                self.write_value(&cell.get(), depth + 1)?;
                self.active_references.pop();
            }
        }
        Ok(())
    }

    fn write_key(&mut self, key: &ArrayKey) {
        match key {
            ArrayKey::Int(value) => self
                .output
                .extend_from_slice(format!("i:{value};").as_bytes()),
            ArrayKey::String(value) => {
                self.output
                    .extend_from_slice(format!("s:{}:\"", value.len()).as_bytes());
                self.output.extend_from_slice(value.as_bytes());
                self.output.extend_from_slice(b"\";");
            }
        }
    }
}

fn serialize_float(value: crate::FloatValue) -> String {
    let value = value.to_f64();
    if value.is_nan() {
        "NAN".to_owned()
    } else if value.is_infinite() {
        if value.is_sign_negative() {
            "-INF".to_owned()
        } else {
            "INF".to_owned()
        }
    } else {
        value.to_string()
    }
}

fn serialized_object_property_name(object: &ObjectRef, storage_name: &str) -> Vec<u8> {
    if let Some((owner, name)) = storage_name
        .strip_prefix("private:")
        .and_then(|rest| rest.split_once(':'))
    {
        let mut serialized = Vec::with_capacity(owner.len() + name.len() + 2);
        serialized.push(0);
        serialized.extend_from_slice(owner.as_bytes());
        serialized.push(0);
        serialized.extend_from_slice(name.as_bytes());
        return serialized;
    }

    let label = object.property_debug_label(storage_name);
    if let Some(name) = label
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix("\":protected"))
    {
        let mut serialized = Vec::with_capacity(name.len() + 3);
        serialized.push(0);
        serialized.push(b'*');
        serialized.push(0);
        serialized.extend_from_slice(name.as_bytes());
        return serialized;
    }

    storage_name.as_bytes().to_vec()
}

struct Parser<'a> {
    bytes: &'a [u8],
    offset: usize,
    options: UnserializeOptions,
    parsed_items: usize,
}

impl Parser<'_> {
    fn parse_value(&mut self, depth: usize) -> Result<Value, SerializationError> {
        if depth > self.options.max_depth {
            return Err(SerializationError::new(
                "serialized value exceeds depth limit",
            ));
        }
        match self.take_byte()? {
            b'N' => {
                self.expect(b';')?;
                Ok(Value::Null)
            }
            b'b' => {
                self.expect(b':')?;
                let value = self.take_bool()?;
                self.expect(b';')?;
                Ok(Value::Bool(value))
            }
            b'i' => {
                self.expect(b':')?;
                let value = self.take_i64_until(b';')?;
                Ok(Value::Int(value))
            }
            b'd' => {
                self.expect(b':')?;
                let value = self.take_f64_until(b';')?;
                Ok(Value::float(value))
            }
            b's' => Ok(Value::String(self.parse_string()?)),
            b'a' => self.parse_array(depth),
            b'O' => self.parse_object(depth),
            b'R' | b'r' => Err(SerializationError::new(
                "serialized reference records are a standard-library known gap",
            )),
            _ => Err(SerializationError::new("unsupported serialized type tag")),
        }
    }

    fn parse_array(&mut self, depth: usize) -> Result<Value, SerializationError> {
        self.expect(b':')?;
        let length = self.take_usize_until(b':')?;
        self.count_items(length)?;
        self.expect(b'{')?;
        let mut array = PhpArray::new();
        for _ in 0..length {
            let key = self.parse_value(depth + 1)?;
            let value = self.parse_value(depth + 1)?;
            let key = ArrayKey::from_value_mvp(&key)
                .ok_or_else(|| SerializationError::new("invalid serialized array key"))?;
            array.insert(key, value);
        }
        self.expect(b'}')?;
        Ok(Value::Array(array))
    }

    fn parse_object(&mut self, depth: usize) -> Result<Value, SerializationError> {
        self.expect(b':')?;
        let class_len = self.take_usize_until(b':')?;
        let class = self.take_quoted_bytes(class_len)?;
        self.expect(b':')?;
        let length = self.take_usize_until(b':')?;
        self.count_items(length)?;
        self.expect(b'{')?;
        let class_name = String::from_utf8_lossy(&class).into_owned();
        let object = ObjectRef::new_with_display_name(
            &empty_class(&class_name),
            display_class_name(&class_name),
        );
        for _ in 0..length {
            let Value::String(name) = self.parse_value(depth + 1)? else {
                return Err(SerializationError::new(
                    "serialized object property name must be a string",
                ));
            };
            let property = self.parse_value(depth + 1)?;
            object.set_property(name.to_string_lossy(), property);
        }
        self.expect(b'}')?;
        Ok(Value::Object(object))
    }

    fn parse_string(&mut self) -> Result<PhpString, SerializationError> {
        self.expect(b':')?;
        let length = self.take_usize_until(b':')?;
        let value = self.take_quoted_bytes(length)?;
        self.expect(b';')?;
        Ok(PhpString::from_bytes(value))
    }

    fn take_quoted_bytes(&mut self, length: usize) -> Result<Vec<u8>, SerializationError> {
        self.expect(b'"')?;
        if self.offset + length > self.bytes.len() {
            return Err(SerializationError::new(
                "serialized string length exceeds input",
            ));
        }
        let value = self.bytes[self.offset..self.offset + length].to_vec();
        self.offset += length;
        self.expect(b'"')?;
        Ok(value)
    }

    fn count_items(&mut self, count: usize) -> Result<(), SerializationError> {
        self.parsed_items = self
            .parsed_items
            .checked_add(count)
            .ok_or_else(|| SerializationError::new("serialized item count overflow"))?;
        if self.parsed_items > self.options.max_items {
            return Err(SerializationError::new(
                "serialized input exceeds item limit",
            ));
        }
        Ok(())
    }

    fn take_byte(&mut self) -> Result<u8, SerializationError> {
        let byte = self
            .bytes
            .get(self.offset)
            .copied()
            .ok_or_else(|| SerializationError::new("unexpected end of serialized input"))?;
        self.offset += 1;
        Ok(byte)
    }

    fn expect(&mut self, expected: u8) -> Result<(), SerializationError> {
        let actual = self.take_byte()?;
        if actual == expected {
            Ok(())
        } else {
            Err(SerializationError::new(format!(
                "expected byte `{}` in serialized input",
                expected as char
            )))
        }
    }

    fn take_bool(&mut self) -> Result<bool, SerializationError> {
        match self.take_byte()? {
            b'0' => Ok(false),
            b'1' => Ok(true),
            _ => Err(SerializationError::new("invalid serialized bool")),
        }
    }

    fn take_i64_until(&mut self, delimiter: u8) -> Result<i64, SerializationError> {
        self.take_ascii_until(delimiter)?
            .parse::<i64>()
            .map_err(|_| SerializationError::new("invalid serialized integer"))
    }

    fn take_usize_until(&mut self, delimiter: u8) -> Result<usize, SerializationError> {
        self.take_ascii_until(delimiter)?
            .parse::<usize>()
            .map_err(|_| SerializationError::new("invalid serialized length"))
    }

    fn take_f64_until(&mut self, delimiter: u8) -> Result<f64, SerializationError> {
        self.take_ascii_until(delimiter)?
            .parse::<f64>()
            .map_err(|_| SerializationError::new("invalid serialized float"))
    }

    fn take_ascii_until(&mut self, delimiter: u8) -> Result<String, SerializationError> {
        let start = self.offset;
        while self.offset < self.bytes.len() && self.bytes[self.offset] != delimiter {
            self.offset += 1;
        }
        if self.offset >= self.bytes.len() {
            return Err(SerializationError::new("unterminated serialized scalar"));
        }
        let text = std::str::from_utf8(&self.bytes[start..self.offset])
            .map_err(|_| SerializationError::new("serialized scalar is not ASCII"))?
            .to_owned();
        self.offset += 1;
        Ok(text)
    }
}

fn empty_class(name: &str) -> ClassEntry {
    ClassEntry {
        name: normalize_class_name(name),
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
    }
}

#[cfg(test)]
mod tests {
    use super::{UnserializeOptions, serialize, unserialize};
    use crate::{
        ClassEntry, ClassFlags, ClassPropertyEntry, ClassPropertyFlags, ClassPropertyHooks,
        ObjectRef, PhpArray, ReferenceCell, Value,
    };

    #[test]
    fn serializes_scalars_arrays_objects_and_references() {
        assert_eq!(serialize(&Value::Null).unwrap().to_string_lossy(), "N;");
        assert_eq!(
            serialize(&Value::Bool(true)).unwrap().to_string_lossy(),
            "b:1;"
        );
        assert_eq!(serialize(&Value::Int(7)).unwrap().to_string_lossy(), "i:7;");
        assert_eq!(
            serialize(&Value::float(f64::INFINITY))
                .unwrap()
                .to_string_lossy(),
            "d:INF;"
        );
        assert_eq!(
            serialize(&Value::float(f64::NEG_INFINITY))
                .unwrap()
                .to_string_lossy(),
            "d:-INF;"
        );
        assert_eq!(
            serialize(&Value::float(f64::NAN))
                .unwrap()
                .to_string_lossy(),
            "d:NAN;"
        );
        assert_eq!(
            serialize(&Value::string("hi")).unwrap().to_string_lossy(),
            "s:2:\"hi\";"
        );
        assert_eq!(
            serialize(&Value::packed_array(vec![
                Value::Int(1),
                Value::string("x")
            ]))
            .unwrap()
            .to_string_lossy(),
            "a:2:{i:0;i:1;i:1;s:1:\"x\";}"
        );

        let object = ObjectRef::new_with_display_name(&super::empty_class("Box"), "Box");
        object.set_property("value", Value::Int(1));
        assert_eq!(
            serialize(&Value::Object(object)).unwrap().to_string_lossy(),
            "O:3:\"Box\":1:{s:5:\"value\";i:1;}"
        );

        let reference = Value::Reference(ReferenceCell::new(Value::Int(9)));
        assert_eq!(serialize(&reference).unwrap().to_string_lossy(), "i:9;");
    }

    #[test]
    fn serializes_object_visibility_property_names() {
        let class = ClassEntry {
            name: "bar".to_owned(),
            parent: Some("foo".to_owned()),
            interfaces: Vec::new(),
            methods: Vec::new(),
            properties: vec![
                ClassPropertyEntry {
                    name: "private:foo:private".to_owned(),
                    default: Value::string("private"),
                    type_: None,
                    flags: ClassPropertyFlags {
                        is_private: true,
                        ..ClassPropertyFlags::default()
                    },
                    hooks: ClassPropertyHooks::default(),
                    attributes: Vec::new(),
                },
                ClassPropertyEntry {
                    name: "protected".to_owned(),
                    default: Value::string("protected"),
                    type_: None,
                    flags: ClassPropertyFlags {
                        is_protected: true,
                        ..ClassPropertyFlags::default()
                    },
                    hooks: ClassPropertyHooks::default(),
                    attributes: Vec::new(),
                },
                ClassPropertyEntry {
                    name: "public".to_owned(),
                    default: Value::string("public"),
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
        let object = ObjectRef::new_with_display_name(&class, "bar");
        let serialized = serialize(&Value::Object(object)).unwrap();

        assert_eq!(
            serialized.to_string_lossy().replace('\0', "\\0"),
            "O:3:\"bar\":3:{s:12:\"\\0foo\\0private\";s:7:\"private\";s:12:\"\\0*\\0protected\";s:9:\"protected\";s:6:\"public\";s:6:\"public\";}"
        );
    }

    #[test]
    fn unserializes_scalars_arrays_and_objects() {
        assert_eq!(
            unserialize(
                &crate::PhpString::from_test_str("a:2:{i:0;i:1;i:1;s:1:\"x\";}"),
                UnserializeOptions::default(),
            )
            .unwrap(),
            Value::packed_array(vec![Value::Int(1), Value::string("x")])
        );

        let value = unserialize(
            &crate::PhpString::from_test_str("O:3:\"Box\":1:{s:5:\"value\";i:1;}"),
            UnserializeOptions::default(),
        )
        .unwrap();
        let Value::Object(object) = value else {
            panic!("expected object");
        };
        assert_eq!(object.class_name(), "box");
        assert_eq!(object.display_name(), "Box");
        assert_eq!(object.get_property("value"), Some(Value::Int(1)));
    }

    #[test]
    fn unserialize_rejects_malformed_and_limited_inputs() {
        assert!(
            unserialize(
                &crate::PhpString::from_test_str("s:99:\"x\";"),
                UnserializeOptions::default(),
            )
            .is_err()
        );
        assert!(
            unserialize(
                &crate::PhpString::from_test_str("a:1:{i:0;i:1;}"),
                UnserializeOptions {
                    max_items: 0,
                    ..UnserializeOptions::default()
                },
            )
            .is_err()
        );

        let cell = ReferenceCell::new(Value::Null);
        let mut array = PhpArray::new();
        array.append(Value::Reference(cell.clone()));
        cell.set(Value::Array(array));
        assert_eq!(
            serialize(&Value::Reference(cell))
                .unwrap()
                .to_string_lossy(),
            "a:1:{i:0;N;}"
        );
    }
}

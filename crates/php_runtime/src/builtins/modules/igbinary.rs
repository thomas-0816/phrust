//! igbinary extension compatibility slice.

use super::core::{argument_type_error, arity_error, string_arg};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use crate::{ArrayKey, PhpArray, PhpString, Value};
use std::collections::HashMap;

const HEADER: [u8; 4] = [0x00, 0x00, 0x00, 0x02];
const MAX_DEPTH: usize = 128;

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new(
        "igbinary_serialize",
        builtin_igbinary_serialize,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "igbinary_unserialize",
        builtin_igbinary_unserialize,
        BuiltinCompatibility::Php,
    ),
];

pub fn serialize_value(value: &Value) -> Result<PhpString, String> {
    let mut encoder = Encoder::new();
    encoder
        .encode_root(value)
        .map_err(|error| error.message().to_owned())?;
    Ok(PhpString::from_bytes(encoder.into_bytes()))
}

pub fn unserialize_value(input: &PhpString) -> Result<Value, String> {
    let mut decoder = Decoder::new(input.as_bytes());
    match decoder.decode_root() {
        Ok(value) if decoder.is_finished() => Ok(value),
        Ok(_) => Err("extra bytes after igbinary value".to_owned()),
        Err(message) => Err(message),
    }
}

fn builtin_igbinary_serialize(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("igbinary_serialize", "exactly one argument"));
    }
    Ok(Value::String(serialize_value(&args[0]).map_err(
        |message| BuiltinError::new("E_PHP_RUNTIME_IGBINARY_SERIALIZE", message),
    )?))
}

fn builtin_igbinary_unserialize(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("igbinary_unserialize", "exactly one argument"));
    }
    let input = string_arg("igbinary_unserialize", &args[0])?;
    match unserialize_value(&input) {
        Ok(value) => Ok(value),
        Err(message) if message == "extra bytes after igbinary value" => {
            context.php_warning(
                "E_PHP_RUNTIME_IGBINARY_TRAILING_BYTES",
                "igbinary_unserialize(): Extra bytes after igbinary value",
                span,
            );
            Ok(Value::Null)
        }
        Err(message) => {
            context.php_warning(
                "E_PHP_RUNTIME_IGBINARY_UNSERIALIZE",
                format!("igbinary_unserialize(): {message}"),
                span,
            );
            Ok(Value::Null)
        }
    }
}

struct Encoder {
    output: Vec<u8>,
    string_table: HashMap<Vec<u8>, usize>,
}

impl Encoder {
    fn new() -> Self {
        Self {
            output: HEADER.to_vec(),
            string_table: HashMap::new(),
        }
    }

    fn into_bytes(self) -> Vec<u8> {
        self.output
    }

    fn encode_root(&mut self, value: &Value) -> Result<(), BuiltinError> {
        self.encode_value(value, 0, matches!(value, Value::Array(_)))
    }

    fn encode_value(
        &mut self,
        value: &Value,
        depth: usize,
        compact_strings: bool,
    ) -> Result<(), BuiltinError> {
        if depth > MAX_DEPTH {
            return Err(BuiltinError::new(
                "E_PHP_RUNTIME_IGBINARY_DEPTH",
                "igbinary_serialize(): Maximum serialization depth exceeded",
            ));
        }
        match value {
            Value::Null | Value::Uninitialized => self.output.push(0x00),
            Value::Bool(false) => self.output.push(0x04),
            Value::Bool(true) => self.output.push(0x05),
            Value::Int(value) => self.encode_int(*value),
            Value::Float(value) => {
                self.output.push(0x0c);
                self.output
                    .extend_from_slice(&value.to_f64().to_bits().to_be_bytes());
            }
            Value::String(value) => self.encode_string(value.as_bytes(), compact_strings),
            Value::Array(array) => self.encode_array(array, depth)?,
            Value::Reference(cell) => self.encode_value(&cell.get(), depth + 1, compact_strings)?,
            Value::Object(_)
            | Value::Resource(_)
            | Value::Fiber(_)
            | Value::Generator(_)
            | Value::Callable(_) => {
                return Err(argument_type_error(
                    "igbinary_serialize",
                    "#1 ($value)",
                    "igbinary-serializable value",
                    value,
                ));
            }
        }
        Ok(())
    }

    fn encode_int(&mut self, value: i64) {
        let negative = value < 0;
        let magnitude = if negative {
            value.unsigned_abs()
        } else {
            value as u64
        };
        if magnitude <= u8::MAX as u64 {
            self.output.push(if negative { 0x07 } else { 0x06 });
            self.output.push(magnitude as u8);
        } else if magnitude <= u16::MAX as u64 {
            self.output.push(if negative { 0x09 } else { 0x08 });
            self.output
                .extend_from_slice(&(magnitude as u16).to_be_bytes());
        } else if magnitude <= u32::MAX as u64 {
            self.output.push(if negative { 0x0b } else { 0x0a });
            self.output
                .extend_from_slice(&(magnitude as u32).to_be_bytes());
        } else {
            self.output.push(if negative { 0x21 } else { 0x20 });
            self.output.extend_from_slice(&magnitude.to_be_bytes());
        }
    }

    fn encode_string(&mut self, bytes: &[u8], compact_strings: bool) {
        if bytes.is_empty() {
            self.output.push(0x0d);
            return;
        }
        if compact_strings {
            if let Some(id) = self.string_table.get(bytes).copied() {
                self.encode_string_ref(id);
                return;
            }
            let id = self.string_table.len();
            self.string_table.insert(bytes.to_vec(), id);
        }
        self.encode_string_bytes(bytes);
    }

    fn encode_string_ref(&mut self, id: usize) {
        if id <= u8::MAX as usize {
            self.output.push(0x0e);
            self.output.push(id as u8);
        } else if id <= u16::MAX as usize {
            self.output.push(0x0f);
            self.output.extend_from_slice(&(id as u16).to_be_bytes());
        } else {
            self.output.push(0x10);
            self.output.extend_from_slice(&(id as u32).to_be_bytes());
        }
    }

    fn encode_string_bytes(&mut self, bytes: &[u8]) {
        let len = bytes.len();
        if len <= u8::MAX as usize {
            self.output.push(0x11);
            self.output.push(len as u8);
        } else if len <= u16::MAX as usize {
            self.output.push(0x12);
            self.output.extend_from_slice(&(len as u16).to_be_bytes());
        } else {
            let len = u32::try_from(len).expect("igbinary string length exceeds u32");
            self.output.push(0x13);
            self.output.extend_from_slice(&len.to_be_bytes());
        }
        self.output.extend_from_slice(bytes);
    }

    fn encode_array(&mut self, array: &PhpArray, depth: usize) -> Result<(), BuiltinError> {
        let len = array.len();
        if len <= u8::MAX as usize {
            self.output.push(0x14);
            self.output.push(len as u8);
        } else if len <= u16::MAX as usize {
            self.output.push(0x15);
            self.output.extend_from_slice(&(len as u16).to_be_bytes());
        } else {
            let len = u32::try_from(len).expect("igbinary array length exceeds u32");
            self.output.push(0x16);
            self.output.extend_from_slice(&len.to_be_bytes());
        }
        for (key, value) in array.iter() {
            match key {
                ArrayKey::Int(key) => self.encode_int(key),
                ArrayKey::String(key) => self.encode_string(key.as_bytes(), true),
            }
            self.encode_value(value, depth + 1, true)?;
        }
        Ok(())
    }
}

struct Decoder<'a> {
    input: &'a [u8],
    offset: usize,
    string_table: Vec<PhpString>,
}

impl<'a> Decoder<'a> {
    fn new(input: &'a [u8]) -> Self {
        Self {
            input,
            offset: 0,
            string_table: Vec::new(),
        }
    }

    const fn is_finished(&self) -> bool {
        self.offset == self.input.len()
    }

    fn decode_root(&mut self) -> Result<Value, String> {
        let header = self.take_array::<4>()?;
        if header != [0x00, 0x00, 0x00, 0x01] && header != HEADER {
            return Err("invalid igbinary header".to_owned());
        }
        self.decode_value(0)
    }

    fn decode_value(&mut self, depth: usize) -> Result<Value, String> {
        if depth > MAX_DEPTH {
            return Err("maximum serialization depth exceeded".to_owned());
        }
        let marker = self.take_u8()?;
        match marker {
            0x00 => Ok(Value::Null),
            0x04 => Ok(Value::Bool(false)),
            0x05 => Ok(Value::Bool(true)),
            0x06 => Ok(Value::Int(self.take_u8()? as i64)),
            0x07 => Ok(Value::Int(-(self.take_u8()? as i64))),
            0x08 => Ok(Value::Int(self.take_u16()? as i64)),
            0x09 => Ok(Value::Int(-(self.take_u16()? as i64))),
            0x0a => Ok(Value::Int(self.take_u32()? as i64)),
            0x0b => Ok(Value::Int(-(self.take_u32()? as i64))),
            0x0c => Ok(Value::float(f64::from_bits(self.take_u64()?))),
            0x0d => Ok(Value::string(Vec::new())),
            0x0e => {
                let id = self.take_u8()? as usize;
                self.decode_string_ref(id)
            }
            0x0f => {
                let id = self.take_u16()? as usize;
                self.decode_string_ref(id)
            }
            0x10 => {
                let id = self.take_u32()? as usize;
                self.decode_string_ref(id)
            }
            0x11 => {
                let len = self.take_u8()? as usize;
                self.decode_string(len)
            }
            0x12 => {
                let len = self.take_u16()? as usize;
                self.decode_string(len)
            }
            0x13 => {
                let len = self.take_u32()? as usize;
                self.decode_string(len)
            }
            0x14 => {
                let len = self.take_u8()? as usize;
                self.decode_array(len, depth)
            }
            0x15 => {
                let len = self.take_u16()? as usize;
                self.decode_array(len, depth)
            }
            0x16 => {
                let len = self.take_u32()? as usize;
                self.decode_array(len, depth)
            }
            0x20 => self.decode_long64(false),
            0x21 => self.decode_long64(true),
            0x01..=0x03 | 0x17..=0x1f | 0x22..=0x25 => Err(format!(
                "unsupported igbinary reference/object marker 0x{marker:02x}"
            )),
            _ => Err(format!("unsupported igbinary marker 0x{marker:02x}")),
        }
    }

    fn decode_string_ref(&mut self, id: usize) -> Result<Value, String> {
        self.string_table
            .get(id)
            .cloned()
            .map(Value::String)
            .ok_or_else(|| format!("invalid igbinary string reference {id}"))
    }

    fn decode_string(&mut self, len: usize) -> Result<Value, String> {
        let string = PhpString::from_bytes(self.take_bytes(len)?.to_vec());
        self.string_table.push(string.clone());
        Ok(Value::String(string))
    }

    fn decode_array(&mut self, len: usize, depth: usize) -> Result<Value, String> {
        let mut array = PhpArray::new();
        for _ in 0..len {
            let key = self.decode_value(depth + 1)?;
            let key = match key {
                Value::Int(value) => ArrayKey::Int(value),
                Value::String(value) => ArrayKey::from_php_string(value),
                other => {
                    return Err(format!(
                        "unsupported igbinary array key type {}",
                        crate::value_type_name(&other)
                    ));
                }
            };
            let value = self.decode_value(depth + 1)?;
            array.insert(key, value);
        }
        Ok(Value::Array(array))
    }

    fn decode_long64(&mut self, negative: bool) -> Result<Value, String> {
        let magnitude = self.take_u64()?;
        if negative {
            if magnitude == (1u64 << 63) {
                Ok(Value::Int(i64::MIN))
            } else {
                let value = i64::try_from(magnitude)
                    .map_err(|_| "negative igbinary integer exceeds PHP integer range")?;
                Ok(Value::Int(-value))
            }
        } else {
            i64::try_from(magnitude)
                .map(Value::Int)
                .map_err(|_| "igbinary integer exceeds PHP integer range".to_owned())
        }
    }

    fn take_u8(&mut self) -> Result<u8, String> {
        let byte = *self
            .input
            .get(self.offset)
            .ok_or_else(|| "unexpected end of igbinary input".to_owned())?;
        self.offset += 1;
        Ok(byte)
    }

    fn take_u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_be_bytes(self.take_array::<2>()?))
    }

    fn take_u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_be_bytes(self.take_array::<4>()?))
    }

    fn take_u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_be_bytes(self.take_array::<8>()?))
    }

    fn take_array<const N: usize>(&mut self) -> Result<[u8; N], String> {
        let bytes = self.take_bytes(N)?;
        let mut out = [0; N];
        out.copy_from_slice(bytes);
        Ok(out)
    }

    fn take_bytes(&mut self, len: usize) -> Result<&'a [u8], String> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or_else(|| "igbinary length overflow".to_owned())?;
        let bytes = self
            .input
            .get(self.offset..end)
            .ok_or_else(|| "unexpected end of igbinary input".to_owned())?;
        self.offset = end;
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::{Decoder, Encoder};
    use crate::{ArrayKey, PhpArray, PhpString, Value};

    fn encode_hex(value: &Value) -> String {
        let mut encoder = Encoder::new();
        encoder.encode_root(value).unwrap();
        encoder
            .into_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    #[test]
    fn encodes_pecl_documented_array_example() {
        let value = Value::packed_array(vec![Value::string("first"), Value::Bool(true)]);
        assert_eq!(encode_hex(&value), "000000021402060011056669727374060105");
    }

    #[test]
    fn encodes_scalar_tags() {
        assert_eq!(encode_hex(&Value::Null), "0000000200");
        assert_eq!(encode_hex(&Value::Bool(false)), "0000000204");
        assert_eq!(encode_hex(&Value::Bool(true)), "0000000205");
        assert_eq!(encode_hex(&Value::Int(123)), "00000002067b");
        assert_eq!(encode_hex(&Value::Int(-123)), "00000002077b");
        assert_eq!(encode_hex(&Value::string(Vec::new())), "000000020d");
        assert_eq!(
            encode_hex(&Value::string("first")),
            "0000000211056669727374"
        );
    }

    #[test]
    fn decodes_pecl_documented_array_example() {
        let bytes = [
            0x00, 0x00, 0x00, 0x02, 0x14, 0x02, 0x06, 0x00, 0x11, 0x05, b'f', b'i', b'r', b's',
            b't', 0x06, 0x01, 0x05,
        ];
        let mut decoder = Decoder::new(&bytes);
        let value = decoder.decode_root().unwrap();
        assert_eq!(
            value,
            Value::packed_array(vec![Value::string("first"), Value::Bool(true)])
        );
        assert!(decoder.is_finished());
    }

    #[test]
    fn roundtrips_mixed_arrays_with_string_refs() {
        let mut array = PhpArray::new();
        array.insert(
            ArrayKey::String(PhpString::from("name")),
            Value::string("alice"),
        );
        array.insert(
            ArrayKey::String(PhpString::from("again")),
            Value::string("alice"),
        );
        array.insert(ArrayKey::Int(9), Value::float(1.5));
        let value = Value::Array(array);

        let mut encoder = Encoder::new();
        encoder.encode_root(&value).unwrap();
        let encoded = encoder.into_bytes();

        let mut decoder = Decoder::new(&encoded);
        assert_eq!(decoder.decode_root().unwrap(), value);
        assert!(decoder.is_finished());
    }
}

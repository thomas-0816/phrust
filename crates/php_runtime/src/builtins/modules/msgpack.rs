//! MessagePack extension compatibility slice.

use super::core::{argument_type_error, arity_error, string_arg};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use crate::{ArrayKey, PhpArray, PhpString, Value};
use std::io::Write;

const MAX_DEPTH: usize = 128;

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new(
        "msgpack_pack",
        builtin_msgpack_pack,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "msgpack_serialize",
        builtin_msgpack_pack,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "msgpack_unserialize",
        builtin_msgpack_unpack,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "msgpack_unpack",
        builtin_msgpack_unpack,
        BuiltinCompatibility::Php,
    ),
];

pub fn pack_value(value: &Value) -> Result<PhpString, String> {
    let mut output = Vec::new();
    encode_value("msgpack_pack", value, &mut output, 0)
        .map_err(|error| error.message().to_owned())?;
    Ok(PhpString::from_bytes(output))
}

pub fn unpack_value(input: &PhpString) -> Result<Value, String> {
    let mut decoder = Decoder::new(input.as_bytes());
    match decoder.decode_value(0) {
        Ok(value) if decoder.is_finished() => Ok(value),
        Ok(_) => Err("extra bytes after MessagePack value".to_owned()),
        Err(message) => Err(message),
    }
}

fn builtin_msgpack_pack(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("msgpack_pack", "exactly one argument"));
    }
    Ok(Value::String(pack_value(&args[0]).map_err(|message| {
        BuiltinError::new("E_PHP_RUNTIME_MSGPACK_ENCODE", message)
    })?))
}

fn builtin_msgpack_unpack(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("msgpack_unpack", "exactly one argument"));
    }
    let input = string_arg("msgpack_unpack", &args[0])?;
    match unpack_value(&input) {
        Ok(value) => Ok(value),
        Err(message) if message == "extra bytes after MessagePack value" => {
            context.php_warning(
                "E_PHP_RUNTIME_MSGPACK_TRAILING_BYTES",
                "msgpack_unpack(): Extra bytes after MessagePack value",
                span,
            );
            Ok(Value::Bool(false))
        }
        Err(message) => {
            context.php_warning(
                "E_PHP_RUNTIME_MSGPACK_UNPACK",
                format!("msgpack_unpack(): {message}"),
                span,
            );
            Ok(Value::Bool(false))
        }
    }
}

fn encode_value(
    function: &str,
    value: &Value,
    output: &mut Vec<u8>,
    depth: usize,
) -> Result<(), BuiltinError> {
    if depth > MAX_DEPTH {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_MSGPACK_DEPTH",
            format!("{function}(): Maximum serialization depth exceeded"),
        ));
    }
    match value {
        Value::Null | Value::Uninitialized => output.push(0xc0),
        Value::Bool(value) => {
            msgpack_write(rmp::encode::write_bool(output, *value), function)?;
        }
        Value::Int(value) => encode_int(function, *value, output)?,
        Value::Float(value) => {
            msgpack_write(rmp::encode::write_f64(output, value.to_f64()), function)?;
        }
        Value::String(value) => encode_bytes(function, value.as_bytes(), output)?,
        Value::Array(array) => encode_array(function, array, output, depth)?,
        Value::Reference(cell) => encode_value(function, &cell.get(), output, depth + 1)?,
        Value::Object(_)
        | Value::Resource(_)
        | Value::Fiber(_)
        | Value::Generator(_)
        | Value::Callable(_) => {
            return Err(argument_type_error(
                function,
                "#1 ($value)",
                "MessagePack-serializable value",
                value,
            ));
        }
    }
    Ok(())
}

fn msgpack_write<T, E: std::fmt::Display>(
    result: Result<T, E>,
    function: &str,
) -> Result<T, BuiltinError> {
    result.map_err(|error| {
        BuiltinError::new(
            "E_PHP_RUNTIME_MSGPACK_ENCODE",
            format!("{function}(): failed to encode MessagePack value: {error}"),
        )
    })
}

fn encode_int(function: &str, value: i64, output: &mut Vec<u8>) -> Result<(), BuiltinError> {
    msgpack_write(rmp::encode::write_sint(output, value).map(|_| ()), function)
}

fn encode_bytes(function: &str, bytes: &[u8], output: &mut Vec<u8>) -> Result<(), BuiltinError> {
    let len = u32::try_from(bytes.len()).map_err(|_| {
        BuiltinError::new(
            "E_PHP_RUNTIME_MSGPACK_SIZE",
            format!("{function}(): MessagePack string length exceeds u32"),
        )
    })?;
    msgpack_write(rmp::encode::write_str_len(output, len), function)?;
    output.write_all(bytes).map_err(|error| {
        BuiltinError::new(
            "E_PHP_RUNTIME_MSGPACK_ENCODE",
            format!("{function}(): failed to encode MessagePack string bytes: {error}"),
        )
    })
}

fn encode_array(
    function: &str,
    array: &PhpArray,
    output: &mut Vec<u8>,
    depth: usize,
) -> Result<(), BuiltinError> {
    if let Some(values) = array.packed_elements() {
        encode_array_header(function, values.len(), output)?;
        for value in values {
            encode_value(function, value, output, depth + 1)?;
        }
        return Ok(());
    }
    encode_map_header(function, array.len(), output)?;
    for (key, value) in array.iter() {
        match key {
            ArrayKey::Int(key) => encode_int(function, key, output)?,
            ArrayKey::String(key) => encode_bytes(function, key.as_bytes(), output)?,
        }
        encode_value(function, value, output, depth + 1)?;
    }
    Ok(())
}

fn encode_array_header(
    function: &str,
    len: usize,
    output: &mut Vec<u8>,
) -> Result<(), BuiltinError> {
    let len = u32::try_from(len).map_err(|_| {
        BuiltinError::new(
            "E_PHP_RUNTIME_MSGPACK_SIZE",
            format!("{function}(): MessagePack array length exceeds u32"),
        )
    })?;
    msgpack_write(
        rmp::encode::write_array_len(output, len).map(|_| ()),
        function,
    )
}

fn encode_map_header(function: &str, len: usize, output: &mut Vec<u8>) -> Result<(), BuiltinError> {
    let len = u32::try_from(len).map_err(|_| {
        BuiltinError::new(
            "E_PHP_RUNTIME_MSGPACK_SIZE",
            format!("{function}(): MessagePack map length exceeds u32"),
        )
    })?;
    msgpack_write(
        rmp::encode::write_map_len(output, len).map(|_| ()),
        function,
    )
}

struct Decoder<'a> {
    input: &'a [u8],
    offset: usize,
}

impl<'a> Decoder<'a> {
    const fn new(input: &'a [u8]) -> Self {
        Self { input, offset: 0 }
    }

    const fn is_finished(&self) -> bool {
        self.offset == self.input.len()
    }

    fn decode_value(&mut self, depth: usize) -> Result<Value, String> {
        if depth > MAX_DEPTH {
            return Err("maximum serialization depth exceeded".to_owned());
        }
        let marker = self.take_u8()?;
        match marker {
            0x00..=0x7f => Ok(Value::Int(marker as i64)),
            0x80..=0x8f => self.decode_map((marker & 0x0f) as usize, depth),
            0x90..=0x9f => self.decode_array((marker & 0x0f) as usize, depth),
            0xa0..=0xbf => self.decode_string((marker & 0x1f) as usize),
            0xc0 => Ok(Value::Null),
            0xc2 => Ok(Value::Bool(false)),
            0xc3 => Ok(Value::Bool(true)),
            0xc4 => {
                let len = self.take_u8()? as usize;
                self.decode_bin(len)
            }
            0xc5 => {
                let len = self.take_u16()? as usize;
                self.decode_bin(len)
            }
            0xc6 => {
                let len = self.take_u32()? as usize;
                self.decode_bin(len)
            }
            0xca => Ok(Value::float(f32::from_bits(self.take_u32()?) as f64)),
            0xcb => Ok(Value::float(f64::from_bits(self.take_u64()?))),
            0xcc => Ok(Value::Int(self.take_u8()? as i64)),
            0xcd => Ok(Value::Int(self.take_u16()? as i64)),
            0xce => Ok(Value::Int(self.take_u32()? as i64)),
            0xcf => {
                let value = self.take_u64()?;
                i64::try_from(value)
                    .map(Value::Int)
                    .map_err(|_| "unsigned integer exceeds PHP integer range".to_owned())
            }
            0xd0 => Ok(Value::Int(self.take_u8()? as i8 as i64)),
            0xd1 => Ok(Value::Int(self.take_i16()? as i64)),
            0xd2 => Ok(Value::Int(self.take_i32()? as i64)),
            0xd3 => Ok(Value::Int(self.take_i64()?)),
            0xd9 => {
                let len = self.take_u8()? as usize;
                self.decode_string(len)
            }
            0xda => {
                let len = self.take_u16()? as usize;
                self.decode_string(len)
            }
            0xdb => {
                let len = self.take_u32()? as usize;
                self.decode_string(len)
            }
            0xdc => {
                let len = self.take_u16()? as usize;
                self.decode_array(len, depth)
            }
            0xdd => {
                let len = self.take_u32()? as usize;
                self.decode_array(len, depth)
            }
            0xde => {
                let len = self.take_u16()? as usize;
                self.decode_map(len, depth)
            }
            0xdf => {
                let len = self.take_u32()? as usize;
                self.decode_map(len, depth)
            }
            0xe0..=0xff => Ok(Value::Int(marker as i8 as i64)),
            _ => Err(format!("unsupported MessagePack marker 0x{marker:02x}")),
        }
    }

    fn decode_array(&mut self, len: usize, depth: usize) -> Result<Value, String> {
        let mut values = Vec::with_capacity(len);
        for _ in 0..len {
            values.push(self.decode_value(depth + 1)?);
        }
        Ok(Value::Array(PhpArray::from_packed(values)))
    }

    fn decode_map(&mut self, len: usize, depth: usize) -> Result<Value, String> {
        let mut array = PhpArray::new();
        for _ in 0..len {
            let key = self.decode_value(depth + 1)?;
            let key = match key {
                Value::Int(value) => ArrayKey::Int(value),
                Value::String(value) => ArrayKey::from_php_string(value),
                Value::Bool(value) => ArrayKey::Int(i64::from(value)),
                Value::Null => ArrayKey::String(PhpString::from_bytes(Vec::new())),
                other => {
                    return Err(format!(
                        "unsupported MessagePack map key type {}",
                        crate::value_type_name(&other)
                    ));
                }
            };
            let value = self.decode_value(depth + 1)?;
            array.insert(key, value);
        }
        Ok(Value::Array(array))
    }

    fn decode_string(&mut self, len: usize) -> Result<Value, String> {
        Ok(Value::string(self.take_bytes(len)?.to_vec()))
    }

    fn decode_bin(&mut self, len: usize) -> Result<Value, String> {
        Ok(Value::string(self.take_bytes(len)?.to_vec()))
    }

    fn take_u8(&mut self) -> Result<u8, String> {
        let byte = *self
            .input
            .get(self.offset)
            .ok_or_else(|| "unexpected end of MessagePack input".to_owned())?;
        self.offset += 1;
        Ok(byte)
    }

    fn take_u16(&mut self) -> Result<u16, String> {
        let bytes = self.take_array::<2>()?;
        Ok(u16::from_be_bytes(bytes))
    }

    fn take_u32(&mut self) -> Result<u32, String> {
        let bytes = self.take_array::<4>()?;
        Ok(u32::from_be_bytes(bytes))
    }

    fn take_u64(&mut self) -> Result<u64, String> {
        let bytes = self.take_array::<8>()?;
        Ok(u64::from_be_bytes(bytes))
    }

    fn take_i16(&mut self) -> Result<i16, String> {
        let bytes = self.take_array::<2>()?;
        Ok(i16::from_be_bytes(bytes))
    }

    fn take_i32(&mut self) -> Result<i32, String> {
        let bytes = self.take_array::<4>()?;
        Ok(i32::from_be_bytes(bytes))
    }

    fn take_i64(&mut self) -> Result<i64, String> {
        let bytes = self.take_array::<8>()?;
        Ok(i64::from_be_bytes(bytes))
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
            .ok_or_else(|| "MessagePack length overflow".to_owned())?;
        let bytes = self
            .input
            .get(self.offset..end)
            .ok_or_else(|| "unexpected end of MessagePack input".to_owned())?;
        self.offset = end;
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::{Decoder, encode_value};
    use crate::{ArrayKey, PhpArray, PhpString, Value};

    #[test]
    fn encodes_and_decodes_scalars_arrays_and_maps() {
        let mut array = PhpArray::from_packed(vec![Value::Int(1), Value::string("two")]);
        array.insert(ArrayKey::String(PhpString::from("name")), Value::Bool(true));
        let value = Value::Array(array);

        let mut encoded = Vec::new();
        encode_value("msgpack_pack", &value, &mut encoded, 0).unwrap();

        let mut decoder = Decoder::new(&encoded);
        assert_eq!(decoder.decode_value(0).unwrap(), value);
        assert!(decoder.is_finished());
    }

    #[test]
    fn decodes_binary_messagepack_shapes() {
        let bytes = [0x82, 0xa1, b'a', 0x01, 0xa1, b'b', 0x92, 0xc2, 0xc0];
        let mut decoder = Decoder::new(&bytes);
        let value = decoder.decode_value(0).unwrap();
        let Value::Array(array) = value else {
            panic!("expected map-shaped PHP array");
        };
        assert_eq!(array.len(), 2);
    }
}

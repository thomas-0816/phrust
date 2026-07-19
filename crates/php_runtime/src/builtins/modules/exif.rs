//! Bounded EXIF/media helpers for common image metadata checks.

use std::io::Cursor;

use super::core::{
    argument_value_error, arity_error, assign_reference_arg, int_arg, read_file_value,
    resource_arg, string_arg, string_array_key,
};
use super::fileinfo::{image_app_info, image_size, image_type, size_array};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinResult, RuntimeSourceSpan,
};
use crate::{PhpArray, ResourceRef, Value};
use exif::{In, Reader, Tag};

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new(
        "exif_imagetype",
        builtin_exif_imagetype,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "exif_read_data",
        builtin_exif_read_data,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "read_exif_data",
        builtin_read_exif_data,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "exif_tagname",
        builtin_exif_tagname,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "exif_thumbnail",
        builtin_exif_thumbnail,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "getimagesize",
        builtin_getimagesize,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "getimagesizefromstring",
        builtin_getimagesizefromstring,
        BuiltinCompatibility::Php,
    ),
];

fn builtin_exif_imagetype(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("exif_imagetype", "one argument"));
    }
    let path = filename_arg("exif_imagetype", "#1 ($filename)", &args[0])?;
    match read_file_value(context, "exif_imagetype", &path, span)? {
        Value::String(bytes) => {
            Ok(image_type(bytes.as_bytes()).map_or(Value::Bool(false), Value::Int))
        }
        _ => Ok(Value::Bool(false)),
    }
}

fn builtin_getimagesize(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("getimagesize", "one or two argument(s)"));
    }
    assign_reference_arg(args.get(1), Value::Array(PhpArray::new()));
    let path = filename_arg("getimagesize", "#1 ($filename)", &args[0])?;
    match read_file_value(context, "getimagesize", &path, span)? {
        Value::String(bytes) => {
            let bytes = bytes.as_bytes();
            Ok(image_size(bytes)
                .map(|info| {
                    assign_reference_arg(args.get(1), Value::Array(image_app_info(bytes)));
                    Value::Array(size_array(&info))
                })
                .unwrap_or(Value::Bool(false)))
        }
        _ => Ok(Value::Bool(false)),
    }
}

fn builtin_getimagesizefromstring(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("getimagesizefromstring", "one argument"));
    }
    let bytes = string_arg("getimagesizefromstring", &args[0])?;
    Ok(image_size(bytes.as_bytes())
        .map(|info| Value::Array(size_array(&info)))
        .unwrap_or(Value::Bool(false)))
}

fn builtin_exif_read_data(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    read_exif_data_impl(context, args, span, "exif_read_data")
}

fn builtin_read_exif_data(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    read_exif_data_impl(context, args, span, "read_exif_data")
}

fn read_exif_data_impl(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
    function: &str,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 4 {
        return Err(arity_error(function, "one to four argument(s)"));
    }
    let Value::String(bytes) = read_exif_input(context, function, &args[0], span)? else {
        return Ok(Value::Bool(false));
    };
    let bytes = bytes.as_bytes();
    let Some(info) = image_size(bytes) else {
        return Ok(Value::Bool(false));
    };
    let mut array = PhpArray::new();
    insert_int(&mut array, "ImageWidth", info.width);
    insert_int(&mut array, "ImageLength", info.height);
    if let Some(fields) = parse_exif_metadata(bytes) {
        if let Some(value) = fields.orientation {
            insert_int(&mut array, "Orientation", i64::from(value));
        }
        if let Some(value) = fields.date_time {
            insert_string(&mut array, "DateTime", value);
        }
        if let Some(value) = fields.make {
            insert_string(&mut array, "Make", value);
        }
        if let Some(value) = fields.model {
            insert_string(&mut array, "Model", value);
        }
    }
    Ok(Value::Array(array))
}

fn builtin_exif_tagname(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("exif_tagname", "one argument"));
    }
    let tag = int_arg("exif_tagname", &args[0])?;
    Ok(exif_tagname(tag).map_or(Value::Bool(false), Value::string))
}

fn builtin_exif_thumbnail(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 4 {
        return Err(arity_error("exif_thumbnail", "one to four argument(s)"));
    }
    let Value::String(bytes) = read_exif_input(context, "exif_thumbnail", &args[0], span)? else {
        return Ok(Value::Bool(false));
    };
    let Some(thumbnail) = extract_jpeg_exif_thumbnail(bytes.as_bytes()) else {
        return Ok(Value::Bool(false));
    };
    if let Some(info) = image_size(&thumbnail) {
        assign_reference_arg(args.get(1), Value::Int(info.width));
        assign_reference_arg(args.get(2), Value::Int(info.height));
        assign_reference_arg(args.get(3), Value::Int(info.image_type));
    }
    Ok(Value::string(thumbnail))
}

fn filename_arg(name: &str, argument: &str, value: &Value) -> Result<String, crate::BuiltinError> {
    let path = string_arg(name, value)?.to_string_lossy();
    if path.is_empty() {
        return Err(argument_value_error(name, argument, "must not be empty"));
    }
    if path.as_bytes().contains(&0) {
        return Err(argument_value_error(
            name,
            argument,
            "must not contain any null bytes",
        ));
    }
    Ok(path)
}

fn read_exif_input(
    context: &mut BuiltinContext<'_>,
    function: &str,
    value: &Value,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if let Some(resource) = resource_arg(value) {
        return read_resource_bytes(function, &resource, context, span);
    }
    let path = filename_arg(function, "#1 ($file)", value)?;
    read_file_value(context, function, &path, span)
}

fn read_resource_bytes(
    function: &str,
    resource: &ResourceRef,
    context: &mut BuiltinContext<'_>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    let original_cursor = resource.tell().ok();
    if let Some(cursor) = original_cursor {
        let _ = resource.seek(0);
        let bytes = resource.read_to_end();
        let _ = resource.seek(cursor);
        return match bytes {
            Ok(bytes) => Ok(Value::string(bytes)),
            Err(error) => {
                context.php_warning(
                    error.diagnostic_id(),
                    format!("{function}(): Failed to read stream: {}", error.message()),
                    span,
                );
                Ok(Value::Bool(false))
            }
        };
    }
    match resource.read_to_end() {
        Ok(bytes) => Ok(Value::string(bytes)),
        Err(error) => {
            context.php_warning(
                error.diagnostic_id(),
                format!("{function}(): Failed to read stream: {}", error.message()),
                span,
            );
            Ok(Value::Bool(false))
        }
    }
}

#[derive(Default)]
struct ExifFields {
    orientation: Option<u16>,
    date_time: Option<String>,
    make: Option<String>,
    model: Option<String>,
}

fn parse_exif_metadata(bytes: &[u8]) -> Option<ExifFields> {
    let mut cursor = Cursor::new(bytes);
    let exif = Reader::new().read_from_container(&mut cursor).ok()?;
    Some(ExifFields {
        orientation: exif_uint_field(&exif, Tag::Orientation)
            .and_then(|value| value.try_into().ok()),
        date_time: exif_ascii_field(&exif, Tag::DateTime),
        make: exif_ascii_field(&exif, Tag::Make),
        model: exif_ascii_field(&exif, Tag::Model),
    })
}

fn exif_uint_field(exif: &exif::Exif, tag: Tag) -> Option<u32> {
    exif.get_field(tag, In::PRIMARY)
        .or_else(|| exif.get_field(tag, In::THUMBNAIL))
        .and_then(|field| field.value.get_uint(0))
}

fn exif_ascii_field(exif: &exif::Exif, tag: Tag) -> Option<String> {
    let field = exif
        .get_field(tag, In::PRIMARY)
        .or_else(|| exif.get_field(tag, In::THUMBNAIL))?;
    let exif::Value::Ascii(values) = &field.value else {
        return None;
    };
    values.iter().find_map(|value| {
        let trimmed = value
            .iter()
            .copied()
            .take_while(|byte| *byte != 0)
            .collect::<Vec<_>>();
        String::from_utf8(trimmed)
            .ok()
            .filter(|value| !value.is_empty())
    })
}

fn extract_jpeg_exif_thumbnail(bytes: &[u8]) -> Option<Vec<u8>> {
    if !bytes.starts_with(b"\xFF\xD8") {
        return None;
    }
    let mut offset = 2usize;
    while offset + 4 <= bytes.len() {
        if bytes[offset] != 0xFF {
            offset += 1;
            continue;
        }
        let marker = bytes[offset + 1];
        offset += 2;
        if marker == 0xD9 || marker == 0xDA || offset + 2 > bytes.len() {
            return None;
        }
        let len = u16::from_be_bytes(bytes[offset..offset + 2].try_into().ok()?) as usize;
        if len < 2 || offset + len > bytes.len() {
            return None;
        }
        let segment = &bytes[offset + 2..offset + len];
        if marker == 0xE1 && segment.starts_with(b"Exif\0\0") {
            return extract_tiff_thumbnail(&segment[6..]);
        }
        offset += len;
    }
    None
}

fn extract_tiff_thumbnail(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.len() < 8 {
        return None;
    }
    let endian = match &bytes[0..2] {
        b"II" => Endian::Little,
        b"MM" => Endian::Big,
        _ => return None,
    };
    if read_u16(bytes, 2, endian)? != 42 {
        return None;
    }
    let ifd0_offset = read_u32(bytes, 4, endian)? as usize;
    let ifd1_offset = next_ifd_offset(bytes, ifd0_offset, endian)?;
    if ifd1_offset == 0 {
        return None;
    }
    let tags = read_ifd_tags(bytes, ifd1_offset, endian)?;
    let jpeg_offset = tags.jpeg_interchange_format? as usize;
    let jpeg_length = tags.jpeg_interchange_format_length? as usize;
    if jpeg_length == 0 {
        return None;
    }
    Some(
        bytes
            .get(jpeg_offset..jpeg_offset.checked_add(jpeg_length)?)?
            .to_vec(),
    )
}

#[derive(Default)]
struct IfdTags {
    jpeg_interchange_format: Option<u32>,
    jpeg_interchange_format_length: Option<u32>,
}

fn next_ifd_offset(bytes: &[u8], ifd_offset: usize, endian: Endian) -> Option<usize> {
    let count = read_u16(bytes, ifd_offset, endian)? as usize;
    let entries_end = ifd_offset
        .checked_add(2)?
        .checked_add(count.checked_mul(12)?)?;
    let next_offset = read_u32(bytes, entries_end, endian)? as usize;
    Some(next_offset)
}

fn read_ifd_tags(bytes: &[u8], ifd_offset: usize, endian: Endian) -> Option<IfdTags> {
    let count = read_u16(bytes, ifd_offset, endian)? as usize;
    let mut tags = IfdTags::default();
    for index in 0..count {
        let entry = ifd_offset + 2 + index * 12;
        if entry + 12 > bytes.len() {
            return None;
        }
        let tag = read_u16(bytes, entry, endian)?;
        let ty = read_u16(bytes, entry + 2, endian)?;
        let count = read_u32(bytes, entry + 4, endian)?;
        let value_field = entry + 8;
        match tag {
            0x0201 => {
                tags.jpeg_interchange_format =
                    read_long_value(bytes, value_field, ty, count, endian)
            }
            0x0202 => {
                tags.jpeg_interchange_format_length =
                    read_long_value(bytes, value_field, ty, count, endian)
            }
            _ => {}
        }
    }
    Some(tags)
}

#[derive(Clone, Copy)]
enum Endian {
    Little,
    Big,
}

fn read_u16(bytes: &[u8], offset: usize, endian: Endian) -> Option<u16> {
    let raw: [u8; 2] = bytes.get(offset..offset + 2)?.try_into().ok()?;
    Some(match endian {
        Endian::Little => u16::from_le_bytes(raw),
        Endian::Big => u16::from_be_bytes(raw),
    })
}

fn read_u32(bytes: &[u8], offset: usize, endian: Endian) -> Option<u32> {
    let raw: [u8; 4] = bytes.get(offset..offset + 4)?.try_into().ok()?;
    Some(match endian {
        Endian::Little => u32::from_le_bytes(raw),
        Endian::Big => u32::from_be_bytes(raw),
    })
}

fn read_long_value(
    bytes: &[u8],
    value_field: usize,
    ty: u16,
    count: u32,
    endian: Endian,
) -> Option<u32> {
    if ty != 4 || count == 0 {
        return None;
    }
    read_u32(bytes, value_field, endian)
}

fn insert_int(array: &mut PhpArray, key: &str, value: i64) {
    array.insert(string_array_key(key), Value::Int(value));
}

fn insert_string(array: &mut PhpArray, key: &str, value: String) {
    array.insert(string_array_key(key), Value::string(value.into_bytes()));
}

fn exif_tagname(tag: i64) -> Option<&'static str> {
    if tag < 0 {
        return None;
    }
    Some(match tag {
        0x000B => "ACDComment",
        0x00FE => "NewSubFile",
        0x00FF => "SubFile",
        0x0100 => "ImageWidth",
        0x0101 => "ImageLength",
        0x0102 => "BitsPerSample",
        0x0103 => "Compression",
        0x0106 => "PhotometricInterpretation",
        0x010A => "FillOrder",
        0x010D => "DocumentName",
        0x010E => "ImageDescription",
        0x010F => "Make",
        0x0110 => "Model",
        0x0111 => "StripOffsets",
        0x0112 => "Orientation",
        0x0115 => "SamplesPerPixel",
        0x0116 => "RowsPerStrip",
        0x0117 => "StripByteCounts",
        0x011A => "XResolution",
        0x011B => "YResolution",
        0x011C => "PlanarConfiguration",
        0x0128 => "ResolutionUnit",
        0x0131 => "Software",
        0x0132 => "DateTime",
        0x013B => "Artist",
        0x013E => "WhitePoint",
        0x013F => "PrimaryChromaticities",
        0x0201 => "JPEGInterchangeFormat",
        0x0202 => "JPEGInterchangeFormatLength",
        0x0211 => "YCbCrCoefficients",
        0x0212 => "YCbCrSubSampling",
        0x0213 => "YCbCrPositioning",
        0x0214 => "ReferenceBlackWhite",
        0x8298 => "Copyright",
        0x8769 => "Exif_IFD_Pointer",
        0x8825 => "GPS_IFD_Pointer",
        0xA005 => "InterOperabilityIndex",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn le16(value: u16) -> [u8; 2] {
        value.to_le_bytes()
    }

    fn le32(value: u32) -> [u8; 4] {
        value.to_le_bytes()
    }

    fn be16(value: u16) -> [u8; 2] {
        value.to_be_bytes()
    }

    fn entry(tag: u16, ty: u16, count: u32, value: [u8; 4]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&le16(tag));
        bytes.extend_from_slice(&le16(ty));
        bytes.extend_from_slice(&le32(count));
        bytes.extend_from_slice(&value);
        bytes
    }

    fn jpeg_with_exif() -> Vec<u8> {
        let date = b"2026:06:28 12:00:00\0";
        let ifd_count = 4usize;
        let ifd_end = 8 + 2 + (ifd_count * 12) + 4;
        let mut tiff = Vec::new();
        tiff.extend_from_slice(b"II");
        tiff.extend_from_slice(&le16(42));
        tiff.extend_from_slice(&le32(8));
        tiff.extend_from_slice(&le16(ifd_count as u16));
        tiff.extend_from_slice(&entry(0x0112, 3, 1, [6, 0, 0, 0]));
        tiff.extend_from_slice(&entry(0x010F, 2, 4, *b"PHP\0"));
        tiff.extend_from_slice(&entry(0x0110, 2, 4, *b"MVP\0"));
        tiff.extend_from_slice(&entry(0x0132, 2, date.len() as u32, le32(ifd_end as u32)));
        tiff.extend_from_slice(&le32(0));
        tiff.extend_from_slice(date);

        let mut app1 = b"Exif\0\0".to_vec();
        app1.extend_from_slice(&tiff);
        let mut jpeg = b"\xFF\xD8\xFF\xE1".to_vec();
        jpeg.extend_from_slice(&be16((app1.len() + 2) as u16));
        jpeg.extend_from_slice(&app1);
        jpeg.extend_from_slice(b"\xFF\xC0");
        jpeg.extend_from_slice(&be16(17));
        jpeg.extend_from_slice(b"\x08");
        jpeg.extend_from_slice(&be16(3));
        jpeg.extend_from_slice(&be16(2));
        jpeg.extend_from_slice(b"\x03\x01\x11\x00\x02\x11\x00\x03\x11\x00\xFF\xD9");
        jpeg
    }

    #[test]
    fn exif_metadata_uses_parser_crate_for_common_fields() {
        let fields = parse_exif_metadata(&jpeg_with_exif()).expect("exif fields");
        assert_eq!(fields.orientation, Some(6));
        assert_eq!(fields.make.as_deref(), Some("PHP"));
        assert_eq!(fields.model.as_deref(), Some("MVP"));
        assert_eq!(fields.date_time.as_deref(), Some("2026:06:28 12:00:00"));
    }
}

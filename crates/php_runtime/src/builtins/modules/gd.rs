//! Bounded GD-compatible image helpers for common media flows.

use super::core::{
    argument_type_error, argument_value_error, arity_error, deref_value, int_arg, read_file_value,
    resolve_runtime_path, string_arg, string_array_key,
};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use crate::{ClassEntry, ClassFlags, ObjectRef, PhpArray, Value, normalize_class_name};
use image::codecs::jpeg::JpegEncoder;
use image::imageops::{self, FilterType};
use image::{DynamicImage, GenericImageView, ImageFormat, Rgba, RgbaImage};
use std::fs;
use std::io::Cursor;

const IMG_JPG: i64 = 2;
const IMG_PNG: i64 = 4;
const SUPPORTED_IMAGE_TYPES: i64 = IMG_JPG | IMG_PNG;

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new("gd_info", builtin_gd_info, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "imagealphablending",
        builtin_imagealphablending,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "imagecolorallocate",
        builtin_imagecolorallocate,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "imagecolorallocatealpha",
        builtin_imagecolorallocatealpha,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "imagecolortransparent",
        builtin_imagecolortransparent,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("imagecopy", builtin_imagecopy, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "imagecopymerge",
        builtin_imagecopymerge,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "imagecopyresampled",
        builtin_imagecopyresampled,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "imagecopyresized",
        builtin_imagecopyresized,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "imagecreatefromjpeg",
        builtin_imagecreatefromjpeg,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "imagecreatefrompng",
        builtin_imagecreatefrompng,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "imagecreatefromstring",
        builtin_imagecreatefromstring,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "imagecreatetruecolor",
        builtin_imagecreatetruecolor,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("imagetypes", builtin_imagetypes, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "imagedestroy",
        builtin_imagedestroy,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("imagefill", builtin_imagefill, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "imagefilledrectangle",
        builtin_imagefilledrectangle,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("imageflip", builtin_imageflip, BuiltinCompatibility::Php),
    BuiltinEntry::new("imagejpeg", builtin_imagejpeg, BuiltinCompatibility::Php),
    BuiltinEntry::new("imageline", builtin_imageline, BuiltinCompatibility::Php),
    BuiltinEntry::new("imagepng", builtin_imagepng, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "imagerectangle",
        builtin_imagerectangle,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "imagerotate",
        builtin_imagerotate,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "imagesavealpha",
        builtin_imagesavealpha,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("imagescale", builtin_imagescale, BuiltinCompatibility::Php),
    BuiltinEntry::new("imagesx", builtin_imagesx, BuiltinCompatibility::Php),
    BuiltinEntry::new("imagesy", builtin_imagesy, BuiltinCompatibility::Php),
];

fn builtin_gd_info(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !args.is_empty() {
        return Err(arity_error("gd_info", "no arguments"));
    }
    let mut array = PhpArray::new();
    insert(&mut array, "GD Version", Value::string("phrust bounded-gd"));
    insert(&mut array, "FreeType Support", Value::Bool(false));
    insert(&mut array, "GIF Read Support", Value::Bool(false));
    insert(&mut array, "GIF Create Support", Value::Bool(false));
    insert(&mut array, "JPEG Support", Value::Bool(true));
    insert(&mut array, "PNG Support", Value::Bool(true));
    insert(&mut array, "WebP Support", Value::Bool(false));
    insert(&mut array, "AVIF Support", Value::Bool(false));
    Ok(Value::Array(array))
}

fn builtin_imagetypes(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !args.is_empty() {
        return Err(arity_error("imagetypes", "no arguments"));
    }
    Ok(Value::Int(SUPPORTED_IMAGE_TYPES))
}

fn builtin_imagecreatefromstring(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("imagecreatefromstring", "one argument"));
    }
    let bytes = string_arg("imagecreatefromstring", &args[0])?;
    Ok(decode_image(bytes.as_bytes()).map_or(Value::Bool(false), gd_object_value))
}

fn builtin_imagecreatefromjpeg(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    imagecreatefrom_file(
        context,
        args,
        span,
        "imagecreatefromjpeg",
        ImageFormat::Jpeg,
    )
}

fn builtin_imagecreatefrompng(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    imagecreatefrom_file(context, args, span, "imagecreatefrompng", ImageFormat::Png)
}

fn builtin_imagecreatetruecolor(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("imagecreatetruecolor", "two arguments"));
    }
    let width = int_arg("imagecreatetruecolor", &args[0])?;
    let height = int_arg("imagecreatetruecolor", &args[1])?;
    if width <= 0 || height <= 0 {
        return Ok(Value::Bool(false));
    }
    let image = DynamicImage::ImageRgba8(RgbaImage::from_pixel(
        width as u32,
        height as u32,
        Rgba([0, 0, 0, 255]),
    ));
    Ok(gd_object_value(image))
}

fn builtin_imagecolorallocate(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 4 {
        return Err(arity_error("imagecolorallocate", "four arguments"));
    }
    let _ = gd_object_arg("imagecolorallocate", &args[0])?;
    Ok(Value::Int(php_color(
        int_arg("imagecolorallocate", &args[1])?,
        int_arg("imagecolorallocate", &args[2])?,
        int_arg("imagecolorallocate", &args[3])?,
        0,
    )))
}

fn builtin_imagecolorallocatealpha(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 5 {
        return Err(arity_error("imagecolorallocatealpha", "five arguments"));
    }
    let _ = gd_object_arg("imagecolorallocatealpha", &args[0])?;
    Ok(Value::Int(php_color(
        int_arg("imagecolorallocatealpha", &args[1])?,
        int_arg("imagecolorallocatealpha", &args[2])?,
        int_arg("imagecolorallocatealpha", &args[3])?,
        int_arg("imagecolorallocatealpha", &args[4])?,
    )))
}

fn builtin_imagecolortransparent(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("imagecolortransparent", "one or two arguments"));
    }
    let (_, _, object) = gd_object_arg("imagecolortransparent", &args[0])?;
    if let Some(color_arg) = args.get(1) {
        let color = int_arg("imagecolortransparent", color_arg)?;
        object.set_property("__gd_transparent", Value::Int(color));
        Ok(Value::Int(color))
    } else {
        Ok(object
            .get_property("__gd_transparent")
            .unwrap_or(Value::Int(-1)))
    }
}

fn builtin_imagesx(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("imagesx", "one argument"));
    }
    Ok(Value::Int(gd_object_arg("imagesx", &args[0])?.0 as i64))
}

fn builtin_imagesy(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("imagesy", "one argument"));
    }
    Ok(Value::Int(gd_object_arg("imagesy", &args[0])?.1 as i64))
}

fn builtin_imagecopyresampled(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    copy_resized(args, "imagecopyresampled", FilterType::Triangle)
}

fn builtin_imagecopyresized(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    copy_resized(args, "imagecopyresized", FilterType::Nearest)
}

fn builtin_imagecopy(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 8 {
        return Err(arity_error("imagecopy", "eight arguments"));
    }
    copy_region(args, "imagecopy", 100)
}

fn builtin_imagecopymerge(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 9 {
        return Err(arity_error("imagecopymerge", "nine arguments"));
    }
    let pct = int_arg("imagecopymerge", &args[8])?.clamp(0, 100) as u8;
    copy_region(args, "imagecopymerge", pct)
}

fn builtin_imagefill(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 4 {
        return Err(arity_error("imagefill", "four arguments"));
    }
    let (_, _, object) = gd_object_arg("imagefill", &args[0])?;
    let x = int_arg("imagefill", &args[1])?;
    let y = int_arg("imagefill", &args[2])?;
    let color = rgba_from_php_color(int_arg("imagefill", &args[3])?);
    let mut image = decode_gd_image(&object)?.to_rgba8();
    if x < 0 || y < 0 || x as u32 >= image.width() || y as u32 >= image.height() {
        return Ok(Value::Bool(false));
    }
    flood_fill(&mut image, x as u32, y as u32, color);
    update_gd_object(&object, DynamicImage::ImageRgba8(image))?;
    Ok(Value::Bool(true))
}

fn builtin_imagefilledrectangle(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 6 {
        return Err(arity_error("imagefilledrectangle", "six arguments"));
    }
    let (_, _, object) = gd_object_arg("imagefilledrectangle", &args[0])?;
    let mut image = decode_gd_image(&object)?.to_rgba8();
    let color = rgba_from_php_color(int_arg("imagefilledrectangle", &args[5])?);
    let (x1, y1, x2, y2) = rectangle_bounds(&args, "imagefilledrectangle")?;
    for y in y1..=y2 {
        for x in x1..=x2 {
            put_pixel_if_inside(&mut image, x, y, color);
        }
    }
    update_gd_object(&object, DynamicImage::ImageRgba8(image))?;
    Ok(Value::Bool(true))
}

fn builtin_imagerectangle(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 6 {
        return Err(arity_error("imagerectangle", "six arguments"));
    }
    let (_, _, object) = gd_object_arg("imagerectangle", &args[0])?;
    let mut image = decode_gd_image(&object)?.to_rgba8();
    let color = rgba_from_php_color(int_arg("imagerectangle", &args[5])?);
    let (x1, y1, x2, y2) = rectangle_bounds(&args, "imagerectangle")?;
    draw_line(&mut image, x1, y1, x2, y1, color);
    draw_line(&mut image, x1, y2, x2, y2, color);
    draw_line(&mut image, x1, y1, x1, y2, color);
    draw_line(&mut image, x2, y1, x2, y2, color);
    update_gd_object(&object, DynamicImage::ImageRgba8(image))?;
    Ok(Value::Bool(true))
}

fn builtin_imageline(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 6 {
        return Err(arity_error("imageline", "six arguments"));
    }
    let (_, _, object) = gd_object_arg("imageline", &args[0])?;
    let mut image = decode_gd_image(&object)?.to_rgba8();
    let x1 = int_arg("imageline", &args[1])?;
    let y1 = int_arg("imageline", &args[2])?;
    let x2 = int_arg("imageline", &args[3])?;
    let y2 = int_arg("imageline", &args[4])?;
    let color = rgba_from_php_color(int_arg("imageline", &args[5])?);
    draw_line(&mut image, x1, y1, x2, y2, color);
    update_gd_object(&object, DynamicImage::ImageRgba8(image))?;
    Ok(Value::Bool(true))
}

fn builtin_imagescale(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 4 {
        return Err(arity_error("imagescale", "two to four arguments"));
    }
    let (src_w, src_h, object) = gd_object_arg("imagescale", &args[0])?;
    let width = int_arg("imagescale", &args[1])?;
    let height = args
        .get(2)
        .map(|value| int_arg("imagescale", value))
        .transpose()?
        .unwrap_or(-1);
    const GD_DIMENSION_MAX: i64 = i32::MAX as i64;
    if width > GD_DIMENSION_MAX {
        return Err(argument_value_error(
            "imagescale",
            "#2 ($width)",
            "must be less than or equal to 2147483647",
        ));
    }
    if height > GD_DIMENSION_MAX {
        return Err(argument_value_error(
            "imagescale",
            "#3 ($height)",
            "must be less than or equal to 2147483647",
        ));
    }
    if width <= 0 {
        return Ok(Value::Bool(false));
    }
    let height = if height <= 0 {
        i64::from(src_h)
            .checked_mul(width)
            .map(|scaled| (scaled / i64::from(src_w.max(1))).max(1))
            .filter(|height| *height <= GD_DIMENSION_MAX)
            .ok_or_else(|| {
                argument_value_error(
                    "imagescale",
                    "#2 ($width)",
                    "produces an image height that exceeds the supported range",
                )
            })?
    } else {
        height
    };
    let resized = imageops::resize(
        &decode_gd_image(&object)?.to_rgba8(),
        width as u32,
        height as u32,
        FilterType::Triangle,
    );
    Ok(gd_object_value(DynamicImage::ImageRgba8(resized)))
}

fn builtin_imagerotate(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 4 {
        return Err(arity_error("imagerotate", "two to four arguments"));
    }
    let (_, _, object) = gd_object_arg("imagerotate", &args[0])?;
    let angle = int_arg("imagerotate", &args[1])?.rem_euclid(360);
    let image = decode_gd_image(&object)?.to_rgba8();
    let rotated = match angle {
        0 => image,
        90 => imageops::rotate90(&image),
        180 => imageops::rotate180(&image),
        270 => imageops::rotate270(&image),
        _ => return Ok(Value::Bool(false)),
    };
    Ok(gd_object_value(DynamicImage::ImageRgba8(rotated)))
}

fn builtin_imageflip(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("imageflip", "two arguments"));
    }
    let (_, _, object) = gd_object_arg("imageflip", &args[0])?;
    let mode = int_arg("imageflip", &args[1])?;
    let image = decode_gd_image(&object)?.to_rgba8();
    let flipped = match mode {
        1 => imageops::flip_horizontal(&image),
        2 => imageops::flip_vertical(&image),
        3 => imageops::flip_vertical(&imageops::flip_horizontal(&image)),
        _ => return Ok(Value::Bool(false)),
    };
    update_gd_object(&object, DynamicImage::ImageRgba8(flipped))?;
    Ok(Value::Bool(true))
}

fn builtin_imagealphablending(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    gd_bool_property("imagealphablending", "__gd_alpha_blending", args)
}

fn builtin_imagesavealpha(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    gd_bool_property("imagesavealpha", "__gd_save_alpha", args)
}

fn copy_resized(args: Vec<Value>, name: &str, filter: FilterType) -> BuiltinResult {
    if args.len() != 10 {
        return Err(arity_error(name, "ten arguments"));
    }
    let (_, _, dst) = gd_object_arg(name, &args[0])?;
    let (_, _, src) = gd_object_arg(name, &args[1])?;
    let dst_x = int_arg(name, &args[2])?.max(0) as u32;
    let dst_y = int_arg(name, &args[3])?.max(0) as u32;
    let src_x = int_arg(name, &args[4])?.max(0) as u32;
    let src_y = int_arg(name, &args[5])?.max(0) as u32;
    let dst_w = int_arg(name, &args[6])?;
    let dst_h = int_arg(name, &args[7])?;
    let src_w = int_arg(name, &args[8])?;
    let src_h = int_arg(name, &args[9])?;
    if dst_w <= 0 || dst_h <= 0 || src_w <= 0 || src_h <= 0 {
        return Ok(Value::Bool(false));
    }
    let mut dst_image = decode_gd_image(&dst)?.to_rgba8();
    let src_image = decode_gd_image(&src)?.to_rgba8();
    if src_x >= src_image.width() || src_y >= src_image.height() {
        return Ok(Value::Bool(false));
    }
    let crop_w = (src_w as u32).min(src_image.width() - src_x);
    let crop_h = (src_h as u32).min(src_image.height() - src_y);
    let cropped = imageops::crop_imm(&src_image, src_x, src_y, crop_w, crop_h).to_image();
    let resized = imageops::resize(&cropped, dst_w as u32, dst_h as u32, filter);
    imageops::overlay(&mut dst_image, &resized, i64::from(dst_x), i64::from(dst_y));
    update_gd_object(&dst, DynamicImage::ImageRgba8(dst_image))?;
    Ok(Value::Bool(true))
}

fn copy_region(args: Vec<Value>, name: &str, pct: u8) -> BuiltinResult {
    let (_, _, dst) = gd_object_arg(name, &args[0])?;
    let (_, _, src) = gd_object_arg(name, &args[1])?;
    let dst_x = int_arg(name, &args[2])?;
    let dst_y = int_arg(name, &args[3])?;
    let src_x = int_arg(name, &args[4])?;
    let src_y = int_arg(name, &args[5])?;
    let src_w = int_arg(name, &args[6])?;
    let src_h = int_arg(name, &args[7])?;
    if src_w <= 0 || src_h <= 0 {
        return Ok(Value::Bool(false));
    }
    let mut dst_image = decode_gd_image(&dst)?.to_rgba8();
    let src_image = decode_gd_image(&src)?.to_rgba8();
    for y in 0..src_h {
        for x in 0..src_w {
            let sx = src_x + x;
            let sy = src_y + y;
            let dx = dst_x + x;
            let dy = dst_y + y;
            if sx < 0
                || sy < 0
                || dx < 0
                || dy < 0
                || sx as u32 >= src_image.width()
                || sy as u32 >= src_image.height()
                || dx as u32 >= dst_image.width()
                || dy as u32 >= dst_image.height()
            {
                continue;
            }
            let src_pixel = *src_image.get_pixel(sx as u32, sy as u32);
            let pixel = if pct >= 100 {
                src_pixel
            } else {
                let dst_pixel = *dst_image.get_pixel(dx as u32, dy as u32);
                mix_pixels(dst_pixel, src_pixel, pct)
            };
            dst_image.put_pixel(dx as u32, dy as u32, pixel);
        }
    }
    update_gd_object(&dst, DynamicImage::ImageRgba8(dst_image))?;
    Ok(Value::Bool(true))
}

fn gd_bool_property(name: &str, property: &str, args: Vec<Value>) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error(name, "two arguments"));
    }
    let (_, _, object) = gd_object_arg(name, &args[0])?;
    object.set_property(property, Value::Bool(bool_arg(&args[1])));
    Ok(Value::Bool(true))
}

fn bool_arg(value: &Value) -> bool {
    match deref_value(value) {
        Value::Bool(value) => value,
        Value::Null | Value::Uninitialized => false,
        Value::Int(value) => value != 0,
        Value::Float(value) => value.to_f64() != 0.0,
        Value::String(value) => {
            let bytes = value.as_bytes();
            !bytes.is_empty() && bytes != b"0"
        }
        Value::Array(array) => !array.is_empty(),
        Value::Object(_) | Value::Resource(_) | Value::Fiber(_) | Value::Generator(_) => true,
        Value::Callable(_) => true,
        Value::Reference(_) => unreachable!("deref_value removes references"),
    }
}

fn rectangle_bounds(args: &[Value], name: &str) -> Result<(i64, i64, i64, i64), BuiltinError> {
    let x1 = int_arg(name, &args[1])?;
    let y1 = int_arg(name, &args[2])?;
    let x2 = int_arg(name, &args[3])?;
    let y2 = int_arg(name, &args[4])?;
    Ok((x1.min(x2), y1.min(y2), x1.max(x2), y1.max(y2)))
}

fn php_color(red: i64, green: i64, blue: i64, alpha: i64) -> i64 {
    let red = red.clamp(0, 255);
    let green = green.clamp(0, 255);
    let blue = blue.clamp(0, 255);
    let alpha = alpha.clamp(0, 127);
    (alpha << 24) | (red << 16) | (green << 8) | blue
}

fn rgba_from_php_color(color: i64) -> Rgba<u8> {
    let red = ((color >> 16) & 0xff) as u8;
    let green = ((color >> 8) & 0xff) as u8;
    let blue = (color & 0xff) as u8;
    let php_alpha = ((color >> 24) & 0x7f) as u8;
    let alpha = (((127 - php_alpha) as u16 * 255) / 127) as u8;
    Rgba([red, green, blue, alpha])
}

fn mix_pixels(dst: Rgba<u8>, src: Rgba<u8>, pct: u8) -> Rgba<u8> {
    let pct = u16::from(pct.min(100));
    let inv = 100 - pct;
    Rgba([
        ((u16::from(dst[0]) * inv + u16::from(src[0]) * pct) / 100) as u8,
        ((u16::from(dst[1]) * inv + u16::from(src[1]) * pct) / 100) as u8,
        ((u16::from(dst[2]) * inv + u16::from(src[2]) * pct) / 100) as u8,
        ((u16::from(dst[3]) * inv + u16::from(src[3]) * pct) / 100) as u8,
    ])
}

fn put_pixel_if_inside(image: &mut RgbaImage, x: i64, y: i64, color: Rgba<u8>) {
    if x >= 0 && y >= 0 && (x as u32) < image.width() && (y as u32) < image.height() {
        image.put_pixel(x as u32, y as u32, color);
    }
}

fn draw_line(image: &mut RgbaImage, x1: i64, y1: i64, x2: i64, y2: i64, color: Rgba<u8>) {
    let mut x = x1;
    let mut y = y1;
    let dx = (x2 - x1).abs();
    let sx = if x1 < x2 { 1 } else { -1 };
    let dy = -(y2 - y1).abs();
    let sy = if y1 < y2 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        put_pixel_if_inside(image, x, y, color);
        if x == x2 && y == y2 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

fn flood_fill(image: &mut RgbaImage, x: u32, y: u32, color: Rgba<u8>) {
    let target = *image.get_pixel(x, y);
    if target == color {
        return;
    }
    let mut stack = vec![(x, y)];
    while let Some((cx, cy)) = stack.pop() {
        if *image.get_pixel(cx, cy) != target {
            continue;
        }
        image.put_pixel(cx, cy, color);
        if cx > 0 {
            stack.push((cx - 1, cy));
        }
        if cy > 0 {
            stack.push((cx, cy - 1));
        }
        if cx + 1 < image.width() {
            stack.push((cx + 1, cy));
        }
        if cy + 1 < image.height() {
            stack.push((cx, cy + 1));
        }
    }
}

fn builtin_imagejpeg(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 3 {
        return Err(arity_error("imagejpeg", "one to three argument(s)"));
    }
    let (_, _, object) = gd_object_arg("imagejpeg", &args[0])?;
    let quality = args
        .get(2)
        .map(|value| int_arg("imagejpeg", value))
        .transpose()?
        .unwrap_or(75)
        .clamp(0, 100) as u8;
    let mut bytes = Vec::new();
    let image = decode_gd_image(&object)?;
    JpegEncoder::new_with_quality(&mut bytes, quality)
        .encode_image(&image)
        .map_err(|error| BuiltinError::new("E_PHP_RUNTIME_GD_ENCODE", error.to_string()))?;
    write_or_output_image(context, args.get(1), bytes)
}

fn builtin_imagepng(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 4 {
        return Err(arity_error("imagepng", "one to four argument(s)"));
    }
    let (_, _, object) = gd_object_arg("imagepng", &args[0])?;
    let bytes = gd_bytes(&object)?;
    write_or_output_image(context, args.get(1), bytes)
}

fn builtin_imagedestroy(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("imagedestroy", "one argument"));
    }
    let (_, _, object) = gd_object_arg("imagedestroy", &args[0])?;
    object.set_property("__gd_destroyed", Value::Bool(true));
    Ok(Value::Bool(true))
}

fn imagecreatefrom_file(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
    name: &str,
    format: ImageFormat,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error(name, "one argument"));
    }
    let path = string_arg(name, &args[0])?.to_string_lossy();
    let Value::String(bytes) = read_file_value(context, name, &path, span)? else {
        return Ok(Value::Bool(false));
    };
    Ok(
        image::load_from_memory_with_format(bytes.as_bytes(), format)
            .ok()
            .map_or(Value::Bool(false), gd_object_value),
    )
}

fn decode_image(bytes: &[u8]) -> Option<DynamicImage> {
    image::load_from_memory(bytes).ok()
}

fn gd_object_value(image: DynamicImage) -> Value {
    Value::Object(gd_object(image))
}

fn gd_object(image: DynamicImage) -> ObjectRef {
    let object = ObjectRef::new_with_display_name(&gd_runtime_class(), "GdImage");
    let _ = update_gd_object(&object, image);
    object
}

fn update_gd_object(object: &ObjectRef, image: DynamicImage) -> Result<(), BuiltinError> {
    let (width, height) = image.dimensions();
    let bytes = encode_png(&image)?;
    object.set_property("__gd_width", Value::Int(i64::from(width)));
    object.set_property("__gd_height", Value::Int(i64::from(height)));
    object.set_property("__gd_format", Value::string("png"));
    object.set_property("__gd_bytes", Value::string(bytes));
    object.set_property("__gd_destroyed", Value::Bool(false));
    Ok(())
}

fn gd_object_arg(name: &str, value: &Value) -> Result<(u32, u32, ObjectRef), BuiltinError> {
    let Value::Object(object) = value else {
        return Err(argument_type_error(name, "1", "GdImage", value));
    };
    if normalize_class_name(&object.class_name()) != "gdimage" {
        return Err(argument_type_error(name, "1", "GdImage", value));
    }
    if matches!(
        object.get_property("__gd_destroyed"),
        Some(Value::Bool(true))
    ) {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_GD_DESTROYED",
            format!("{name}(): GdImage object has been destroyed"),
        ));
    }
    let width = match object.get_property("__gd_width") {
        Some(Value::Int(value)) if value > 0 => value as u32,
        _ => 0,
    };
    let height = match object.get_property("__gd_height") {
        Some(Value::Int(value)) if value > 0 => value as u32,
        _ => 0,
    };
    Ok((width, height, object.clone()))
}

fn decode_gd_image(object: &ObjectRef) -> Result<DynamicImage, BuiltinError> {
    image::load_from_memory(&gd_bytes(object)?)
        .map_err(|error| BuiltinError::new("E_PHP_RUNTIME_GD_DECODE", error.to_string()))
}

fn gd_bytes(object: &ObjectRef) -> Result<Vec<u8>, BuiltinError> {
    match object.get_property("__gd_bytes") {
        Some(Value::String(bytes)) => Ok(bytes.as_bytes().to_vec()),
        _ => Err(BuiltinError::new(
            "E_PHP_RUNTIME_GD_STATE",
            "GdImage object is missing image data",
        )),
    }
}

fn encode_png(image: &DynamicImage) -> Result<Vec<u8>, BuiltinError> {
    let mut cursor = Cursor::new(Vec::new());
    image
        .write_to(&mut cursor, ImageFormat::Png)
        .map_err(|error| BuiltinError::new("E_PHP_RUNTIME_GD_ENCODE", error.to_string()))?;
    Ok(cursor.into_inner())
}

fn write_or_output_image(
    context: &mut BuiltinContext<'_>,
    path_arg: Option<&Value>,
    bytes: Vec<u8>,
) -> BuiltinResult {
    match path_arg {
        None | Some(Value::Null) => {
            context.output().write_bytes(&bytes);
            Ok(Value::Bool(true))
        }
        Some(value) => {
            let path = string_arg("image output", value)?.to_string_lossy();
            let resolved = resolve_runtime_path(context, &path);
            if !context.filesystem_capabilities().allows_path(&resolved) {
                return Ok(Value::Bool(false));
            }
            Ok(Value::Bool(fs::write(resolved, bytes).is_ok()))
        }
    }
}

fn gd_runtime_class() -> ClassEntry {
    ClassEntry {
        name: "gdimage".to_owned().into(),
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

fn insert(array: &mut PhpArray, key: &str, value: Value) {
    array.insert(string_array_key(key), value);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OutputBuffer;

    #[test]
    fn imagetypes_reports_bounded_jpeg_and_png_support() {
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);

        assert_eq!(
            builtin_imagetypes(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("imagetypes succeeds"),
            Value::Int(IMG_JPG | IMG_PNG)
        );
    }

    #[test]
    fn gd_info_matches_bounded_image_type_support() {
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);
        let Value::Array(info) =
            builtin_gd_info(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("gd_info succeeds")
        else {
            panic!("expected GD info array");
        };

        assert_eq!(
            info.get(&string_array_key("JPEG Support")),
            Some(&Value::Bool(true))
        );
        assert_eq!(
            info.get(&string_array_key("PNG Support")),
            Some(&Value::Bool(true))
        );
        assert_eq!(
            info.get(&string_array_key("GIF Read Support")),
            Some(&Value::Bool(false))
        );
        assert_eq!(
            info.get(&string_array_key("WebP Support")),
            Some(&Value::Bool(false))
        );
        assert_eq!(
            info.get(&string_array_key("AVIF Support")),
            Some(&Value::Bool(false))
        );
    }

    #[test]
    fn gd_common_drawing_and_transform_functions_mutate_images() {
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);
        let image = builtin_imagecreatetruecolor(
            &mut context,
            vec![Value::Int(4), Value::Int(3)],
            RuntimeSourceSpan::default(),
        )
        .expect("create image");
        let red = builtin_imagecolorallocate(
            &mut context,
            vec![image.clone(), Value::Int(255), Value::Int(0), Value::Int(0)],
            RuntimeSourceSpan::default(),
        )
        .expect("allocate red");
        let blue = builtin_imagecolorallocatealpha(
            &mut context,
            vec![
                image.clone(),
                Value::Int(0),
                Value::Int(0),
                Value::Int(255),
                Value::Int(0),
            ],
            RuntimeSourceSpan::default(),
        )
        .expect("allocate blue");

        assert_eq!(
            builtin_imagefill(
                &mut context,
                vec![image.clone(), Value::Int(0), Value::Int(0), red],
                RuntimeSourceSpan::default(),
            )
            .expect("fill"),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_imagefilledrectangle(
                &mut context,
                vec![
                    image.clone(),
                    Value::Int(1),
                    Value::Int(1),
                    Value::Int(2),
                    Value::Int(2),
                    blue.clone(),
                ],
                RuntimeSourceSpan::default(),
            )
            .expect("filled rectangle"),
            Value::Bool(true)
        );
        let (_, _, object) = gd_object_arg("test", &image).expect("gd object");
        let pixels = decode_gd_image(&object).expect("decode").to_rgba8();
        assert_eq!(pixels.get_pixel(0, 0), &Rgba([255, 0, 0, 255]));
        assert_eq!(pixels.get_pixel(1, 1), &Rgba([0, 0, 255, 255]));

        let scaled = builtin_imagescale(
            &mut context,
            vec![image.clone(), Value::Int(8), Value::Int(-1)],
            RuntimeSourceSpan::default(),
        )
        .expect("scale");
        assert_eq!(
            builtin_imagesx(
                &mut context,
                vec![scaled.clone()],
                RuntimeSourceSpan::default()
            )
            .expect("scaled width"),
            Value::Int(8)
        );
        assert!(matches!(
            builtin_imagerotate(
                &mut context,
                vec![image.clone(), Value::Int(90), blue],
                RuntimeSourceSpan::default(),
            )
            .expect("rotate"),
            Value::Object(_)
        ));
        assert_eq!(
            builtin_imageflip(
                &mut context,
                vec![image.clone(), Value::Int(1)],
                RuntimeSourceSpan::default(),
            )
            .expect("flip"),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_imagesavealpha(
                &mut context,
                vec![image.clone(), Value::Bool(true)],
                RuntimeSourceSpan::default(),
            )
            .expect("save alpha"),
            Value::Bool(true)
        );

        let width_error = builtin_imagescale(
            &mut context,
            vec![image.clone(), Value::Int(i64::MAX), Value::Int(-1)],
            RuntimeSourceSpan::default(),
        )
        .expect_err("oversized width is rejected before image allocation");
        assert_eq!(
            width_error.message(),
            "imagescale(): Argument #2 ($width) must be less than or equal to 2147483647"
        );
        let height_error = builtin_imagescale(
            &mut context,
            vec![image, Value::Int(-1), Value::Int(i64::MAX)],
            RuntimeSourceSpan::default(),
        )
        .expect_err("oversized height is rejected even when width is negative");
        assert_eq!(
            height_error.message(),
            "imagescale(): Argument #3 ($height) must be less than or equal to 2147483647"
        );
    }
}

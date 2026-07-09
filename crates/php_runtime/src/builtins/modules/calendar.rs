//! Bounded calendar extension over php-src serial day number algorithms.

use super::core::{argument_value_error, arity_error, int_arg, value_error};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use crate::{ArrayKey, PhpArray, PhpString, Value, to_bool};

const CAL_GREGORIAN: i64 = 0;
const CAL_JULIAN: i64 = 1;
const CAL_JEWISH: i64 = 2;
const CAL_FRENCH: i64 = 3;
const CAL_NUM_CALS: i64 = 4;

const CAL_DOW_DAYNO: i64 = 0;
const CAL_DOW_LONG: i64 = 1;
const CAL_DOW_SHORT: i64 = 2;

const CAL_MONTH_GREGORIAN_SHORT: i64 = 0;
const CAL_MONTH_GREGORIAN_LONG: i64 = 1;
const CAL_MONTH_JULIAN_SHORT: i64 = 2;
const CAL_MONTH_JULIAN_LONG: i64 = 3;
const CAL_MONTH_JEWISH: i64 = 4;
const CAL_MONTH_FRENCH: i64 = 5;

const CAL_EASTER_DEFAULT: i64 = 0;
const CAL_EASTER_ROMAN: i64 = 1;
const CAL_EASTER_ALWAYS_GREGORIAN: i64 = 2;
const CAL_EASTER_ALWAYS_JULIAN: i64 = 3;
const CAL_JEWISH_ADD_ALAFIM_GERESH: i64 = 2;
const CAL_JEWISH_ADD_ALAFIM: i64 = 4;
const CAL_JEWISH_ADD_GERESHAYIM: i64 = 8;

const SECS_PER_DAY: i64 = 86_400;
const UNIX_EPOCH_SDN: i64 = 2_440_588;
const UNIX_MAX_JD: i64 = i64::MAX / SECS_PER_DAY + UNIX_EPOCH_SDN;
const EASTER_MAX_YEAR: i64 = (i64::MAX / 5) * 4;
const FRENCH_SDN_OFFSET: i64 = 2_375_474;
const FRENCH_FIRST_VALID: i64 = 2_375_840;
const FRENCH_LAST_VALID: i64 = 2_380_952;
const DAYS_PER_4_YEARS: i64 = 1_461;
const DAYS_PER_FRENCH_MONTH: i64 = 30;

const HALAKIM_PER_HOUR: i64 = 1_080;
const HALAKIM_PER_DAY: i64 = 25_920;
const HALAKIM_PER_LUNAR_CYCLE: i64 = 765_433;
const HALAKIM_PER_METONIC_CYCLE: i64 = HALAKIM_PER_LUNAR_CYCLE * (12 * 19 + 7);
const JEWISH_SDN_OFFSET: i64 = 347_997;
const JEWISH_SDN_MAX: i64 = 324_542_846;
const NEW_MOON_OF_CREATION: i64 = 31_524;
const NOON: i64 = 18 * HALAKIM_PER_HOUR;
const AM3_11_20: i64 = 9 * HALAKIM_PER_HOUR + 204;
const AM9_32_43: i64 = 15 * HALAKIM_PER_HOUR + 589;

const SUNDAY: i64 = 0;
const MONDAY: i64 = 1;
const TUESDAY: i64 = 2;
const WEDNESDAY: i64 = 3;
const FRIDAY: i64 = 5;

const JEWISH_MONTHS_PER_YEAR: [i64; 19] = [
    12, 12, 13, 12, 12, 13, 12, 13, 12, 12, 13, 12, 12, 13, 12, 12, 13, 12, 13,
];
const JEWISH_YEAR_OFFSET: [i64; 19] = [
    0, 12, 24, 37, 49, 61, 74, 86, 99, 111, 123, 136, 148, 160, 173, 185, 197, 210, 222,
];

const MONTH_NAME_SHORT: [&str; 13] = [
    "", "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const MONTH_NAME_LONG: [&str; 13] = [
    "",
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const DAY_NAME_SHORT: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const DAY_NAME_LONG: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
const FRENCH_MONTH_NAME: [&str; 14] = [
    "",
    "Vendemiaire",
    "Brumaire",
    "Frimaire",
    "Nivose",
    "Pluviose",
    "Ventose",
    "Germinal",
    "Floreal",
    "Prairial",
    "Messidor",
    "Thermidor",
    "Fructidor",
    "Extra",
];
const JEWISH_MONTH_NAME_LEAP: [&str; 14] = [
    "", "Tishri", "Heshvan", "Kislev", "Tevet", "Shevat", "Adar I", "Adar II", "Nisan", "Iyyar",
    "Sivan", "Tammuz", "Av", "Elul",
];
const JEWISH_MONTH_NAME: [&str; 14] = [
    "", "Tishri", "Heshvan", "Kislev", "Tevet", "Shevat", "", "Adar", "Nisan", "Iyyar", "Sivan",
    "Tammuz", "Av", "Elul",
];
const JEWISH_HEB_MONTH_NAME_LEAP: [&[u8]; 14] = [
    b"",
    b"\xFA\xF9\xF8\xE9",
    b"\xE7\xF9\xE5\xEF",
    b"\xEB\xF1\xEC\xE5",
    b"\xE8\xE1\xFA",
    b"\xF9\xE1\xE8",
    b"\xE0\xE3\xF8 \xE0'",
    b"\xE0\xE3\xF8 \xE1'",
    b"\xF0\xE9\xF1\xEF",
    b"\xE0\xE9\xE9\xF8",
    b"\xF1\xE9\xE5\xEF",
    b"\xFA\xEE\xE5\xE6",
    b"\xE0\xE1",
    b"\xE0\xEC\xE5\xEC",
];
const JEWISH_HEB_MONTH_NAME: [&[u8]; 14] = [
    b"",
    b"\xFA\xF9\xF8\xE9",
    b"\xE7\xF9\xE5\xEF",
    b"\xEB\xF1\xEC\xE5",
    b"\xE8\xE1\xFA",
    b"\xF9\xE1\xE8",
    b"",
    b"\xE0\xE3\xF8",
    b"\xF0\xE9\xF1\xEF",
    b"\xE0\xE9\xE9\xF8",
    b"\xF1\xE9\xE5\xEF",
    b"\xFA\xEE\xE5\xE6",
    b"\xE0\xE1",
    b"\xE0\xEC\xE5\xEC",
];
const HEBREW_NUMBER_LETTERS: [u8; 23] = [
    b'0', 0xE0, 0xE1, 0xE2, 0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEB, 0xEC, 0xEE, 0xF0, 0xF1,
    0xF2, 0xF4, 0xF6, 0xF7, 0xF8, 0xF9, 0xFA,
];

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new(
        "cal_days_in_month",
        builtin_cal_days_in_month,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "cal_from_jd",
        builtin_cal_from_jd,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("cal_info", builtin_cal_info, BuiltinCompatibility::Php),
    BuiltinEntry::new("cal_to_jd", builtin_cal_to_jd, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "easter_date",
        builtin_easter_date,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "easter_days",
        builtin_easter_days,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("frenchtojd", builtin_frenchtojd, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "gregoriantojd",
        builtin_gregoriantojd,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "jddayofweek",
        builtin_jddayofweek,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "jdmonthname",
        builtin_jdmonthname,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("jdtofrench", builtin_jdtofrench, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "jdtogregorian",
        builtin_jdtogregorian,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("jdtojewish", builtin_jdtojewish, BuiltinCompatibility::Php),
    BuiltinEntry::new("jdtojulian", builtin_jdtojulian, BuiltinCompatibility::Php),
    BuiltinEntry::new("jdtounix", builtin_jdtounix, BuiltinCompatibility::Php),
    BuiltinEntry::new("jewishtojd", builtin_jewishtojd, BuiltinCompatibility::Php),
    BuiltinEntry::new("juliantojd", builtin_juliantojd, BuiltinCompatibility::Php),
    BuiltinEntry::new("unixtojd", builtin_unixtojd, BuiltinCompatibility::Php),
];

fn builtin_cal_days_in_month(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(arity_error("cal_days_in_month", "three arguments"));
    }
    let cal = calendar_arg("cal_days_in_month", 1, &args[0])?;
    let month = bounded_positive_i32_arg("cal_days_in_month", 2, &args[1])?;
    let year = bounded_less_than_i32_max_arg("cal_days_in_month", 3, &args[2])?;
    let start = calendar_to_sdn(cal, year, month, 1)?;
    if start == 0 {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_VALUE",
            "Invalid date",
        ));
    }
    let mut next = calendar_to_sdn(cal, year, month + 1, 1)?;
    if next == 0 {
        next = calendar_to_sdn(cal, if year == -1 { 1 } else { year + 1 }, 1, 1)?;
        if cal == CAL_FRENCH && next == 0 {
            next = FRENCH_LAST_VALID + 1;
        }
    }
    if next == 0 {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_VALUE",
            "Invalid date",
        ));
    }
    Ok(Value::Int(next - start))
}

fn builtin_cal_to_jd(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 4 {
        return Err(arity_error("cal_to_jd", "four arguments"));
    }
    let cal = calendar_arg("cal_to_jd", 1, &args[0])?;
    let month = bounded_positive_i32_arg("cal_to_jd", 2, &args[1])?;
    let day = bounded_i32_arg("cal_to_jd", 3, &args[2])?;
    let year = bounded_less_than_i32_max_arg("cal_to_jd", 4, &args[3])?;
    Ok(Value::Int(calendar_to_sdn(cal, year, month, day)?))
}

fn builtin_cal_from_jd(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("cal_from_jd", "two arguments"));
    }
    let jd = int_arg("cal_from_jd", &args[0])?;
    let cal = calendar_arg("cal_from_jd", 2, &args[1])?;
    let (year, month, day) = calendar_from_sdn(cal, jd)?;
    let dow = day_of_week(jd);
    let invalid_jewish_date = cal == CAL_JEWISH && year == 0 && month == 0 && day == 0;
    let mut result = PhpArray::new();
    result.insert(
        string_key("date"),
        Value::string(format!("{month}/{day}/{year}")),
    );
    result.insert(string_key("month"), Value::Int(month));
    result.insert(string_key("day"), Value::Int(day));
    result.insert(string_key("year"), Value::Int(year));
    result.insert(
        string_key("dow"),
        if invalid_jewish_date {
            Value::Null
        } else {
            Value::Int(dow)
        },
    );
    result.insert(
        string_key("abbrevdayname"),
        Value::string(if invalid_jewish_date {
            ""
        } else {
            DAY_NAME_SHORT[dow as usize]
        }),
    );
    result.insert(
        string_key("dayname"),
        Value::string(if invalid_jewish_date {
            ""
        } else {
            DAY_NAME_LONG[dow as usize]
        }),
    );
    result.insert(
        string_key("abbrevmonth"),
        Value::string(month_name(cal, month, false)?),
    );
    result.insert(
        string_key("monthname"),
        Value::string(month_name(cal, month, true)?),
    );
    Ok(Value::Array(result))
}

fn builtin_cal_info(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error("cal_info", "at most one argument"));
    }
    if let Some(value) = args.first() {
        let cal = int_arg("cal_info", value)?;
        let cal = valid_calendar("cal_info", 1, cal)?;
        return Ok(Value::Array(cal_info_array(cal)));
    }
    let mut result = PhpArray::new();
    for cal in CAL_GREGORIAN..CAL_NUM_CALS {
        result.insert(ArrayKey::Int(cal), Value::Array(cal_info_array(cal)));
    }
    Ok(Value::Array(result))
}

fn builtin_gregoriantojd(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(arity_error("gregoriantojd", "three arguments"));
    }
    let month = bounded_i32_arg("gregoriantojd", 1, &args[0])?;
    let day = bounded_i32_arg("gregoriantojd", 2, &args[1])?;
    let year = bounded_i32_arg("gregoriantojd", 3, &args[2])?;
    Ok(Value::Int(gregorian_to_sdn(year, month, day)))
}

fn builtin_jdtogregorian(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("jdtogregorian", "one argument"));
    }
    let jd = int_arg("jdtogregorian", &args[0])?;
    let (year, month, day) = sdn_to_gregorian(jd);
    Ok(Value::string(format!("{month}/{day}/{year}")))
}

fn builtin_juliantojd(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(arity_error("juliantojd", "three arguments"));
    }
    let month = bounded_i32_arg("juliantojd", 1, &args[0])?;
    let day = bounded_i32_arg("juliantojd", 2, &args[1])?;
    let year = int_arg("juliantojd", &args[2])? as i32 as i64;
    Ok(Value::Int(julian_to_sdn(year, month, day)))
}

fn builtin_jdtojulian(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("jdtojulian", "one argument"));
    }
    let jd = int_arg("jdtojulian", &args[0])?;
    let (year, month, day) = sdn_to_julian(jd);
    Ok(Value::string(format!("{month}/{day}/{year}")))
}

fn builtin_jddayofweek(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("jddayofweek", "one or two arguments"));
    }
    let jd = int_arg("jddayofweek", &args[0])?;
    let mode = args
        .get(1)
        .map(|value| int_arg("jddayofweek", value))
        .transpose()?
        .unwrap_or(CAL_DOW_DAYNO);
    let dow = day_of_week(jd) as usize;
    match mode {
        CAL_DOW_LONG => Ok(Value::string(DAY_NAME_LONG[dow])),
        CAL_DOW_SHORT => Ok(Value::string(DAY_NAME_SHORT[dow])),
        _ => Ok(Value::Int(dow as i64)),
    }
}

fn builtin_jdmonthname(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("jdmonthname", "two arguments"));
    }
    let jd = int_arg("jdmonthname", &args[0])?;
    let mode = int_arg("jdmonthname", &args[1])?;
    let name = match mode {
        CAL_MONTH_GREGORIAN_LONG => month_name_by_index(&MONTH_NAME_LONG, sdn_to_gregorian(jd).1),
        CAL_MONTH_JULIAN_SHORT => month_name_by_index(&MONTH_NAME_SHORT, sdn_to_julian(jd).1),
        CAL_MONTH_JULIAN_LONG => month_name_by_index(&MONTH_NAME_LONG, sdn_to_julian(jd).1),
        CAL_MONTH_FRENCH => month_name_by_index(&FRENCH_MONTH_NAME, sdn_to_french(jd).1),
        CAL_MONTH_JEWISH => {
            let (year, month, _) = sdn_to_jewish(jd);
            jewish_month_name(year, month)
        }
        CAL_MONTH_GREGORIAN_SHORT => month_name_by_index(&MONTH_NAME_SHORT, sdn_to_gregorian(jd).1),
        _ => month_name_by_index(&MONTH_NAME_SHORT, sdn_to_gregorian(jd).1),
    };
    Ok(Value::string(name))
}

fn builtin_jdtounix(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("jdtounix", "one argument"));
    }
    let jd = int_arg("jdtounix", &args[0])?;
    if jd < UNIX_EPOCH_SDN || jd - UNIX_EPOCH_SDN > i64::MAX / SECS_PER_DAY {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_VALUE",
            format!("jday must be between {UNIX_EPOCH_SDN} and {UNIX_MAX_JD}"),
        ));
    }
    Ok(Value::Int((jd - UNIX_EPOCH_SDN) * SECS_PER_DAY))
}

fn builtin_unixtojd(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error("unixtojd", "at most one argument"));
    }
    let timestamp = args
        .first()
        .map(|value| int_arg("unixtojd", value))
        .transpose()?
        .unwrap_or(0);
    if timestamp < 0 {
        return Err(argument_value_error(
            "unixtojd",
            "#1 ($timestamp)",
            "must be greater than or equal to 0",
        ));
    }
    Ok(Value::Int(UNIX_EPOCH_SDN + timestamp / SECS_PER_DAY))
}

fn builtin_easter_days(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 2 {
        return Err(arity_error("easter_days", "at most two arguments"));
    }
    let year = args
        .first()
        .map(|value| int_arg("easter_days", value))
        .transpose()?
        .unwrap_or(1970);
    let method = args
        .get(1)
        .map(|value| int_arg("easter_days", value))
        .transpose()?
        .unwrap_or(CAL_EASTER_DEFAULT);
    validate_easter_year("easter_days", year, false)?;
    Ok(Value::Int(easter_days(year, method)))
}

fn builtin_easter_date(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 2 {
        return Err(arity_error("easter_date", "at most two arguments"));
    }
    let year = args
        .first()
        .map(|value| int_arg("easter_date", value))
        .transpose()?
        .unwrap_or(1970);
    let method = args
        .get(1)
        .map(|value| int_arg("easter_date", value))
        .transpose()?
        .unwrap_or(CAL_EASTER_DEFAULT);
    validate_easter_year("easter_date", year, true)?;
    let days_after_march_21 = easter_days(year, method);
    let (month, day) = if days_after_march_21 < 11 {
        (3, days_after_march_21 + 21)
    } else {
        (4, days_after_march_21 - 10)
    };
    let jd = gregorian_to_sdn(year, month, day);
    Ok(Value::Int((jd - UNIX_EPOCH_SDN) * SECS_PER_DAY))
}

fn builtin_jewishtojd(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(arity_error("jewishtojd", "three arguments"));
    }
    let month = bounded_i32_arg("jewishtojd", 1, &args[0])?;
    let day = bounded_i32_arg("jewishtojd", 2, &args[1])?;
    let year = bounded_i32_arg("jewishtojd", 3, &args[2])?;
    Ok(Value::Int(jewish_to_sdn(year, month, day)))
}

fn builtin_jdtojewish(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 3 {
        return Err(arity_error("jdtojewish", "one to three arguments"));
    }
    let jd = int_arg("jdtojewish", &args[0])?;
    let hebrew = args
        .get(1)
        .map(to_bool)
        .transpose()
        .map_err(|message| value_error("jdtojewish", &message))?
        .unwrap_or(false);
    let flags = args
        .get(2)
        .map(|value| int_arg("jdtojewish", value))
        .transpose()?
        .unwrap_or(0);
    let (year, month, day) = sdn_to_jewish(jd);
    if hebrew {
        if year <= 0 || year > 9999 {
            return Err(value_error("jdtojewish", "Year out of range (0-9999)"));
        }
        let Some(day_text) = hebrew_number_to_chars(day, flags) else {
            return Err(value_error("jdtojewish", "Day out of range (1-9999)"));
        };
        let Some(year_text) = hebrew_number_to_chars(year, flags) else {
            return Err(value_error("jdtojewish", "Year out of range (0-9999)"));
        };
        let mut formatted = Vec::new();
        formatted.extend_from_slice(&day_text);
        formatted.push(b' ');
        formatted.extend_from_slice(jewish_hebrew_month_name(year, month));
        formatted.push(b' ');
        formatted.extend_from_slice(&year_text);
        return Ok(Value::string(formatted));
    }
    Ok(Value::string(format!("{month}/{day}/{year}")))
}

fn builtin_frenchtojd(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(arity_error("frenchtojd", "three arguments"));
    }
    let month = bounded_i32_arg("frenchtojd", 1, &args[0])?;
    let day = bounded_i32_arg("frenchtojd", 2, &args[1])?;
    let year = bounded_i32_arg("frenchtojd", 3, &args[2])?;
    Ok(Value::Int(french_to_sdn(year, month, day)))
}

fn builtin_jdtofrench(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("jdtofrench", "one argument"));
    }
    let jd = int_arg("jdtofrench", &args[0])?;
    let (year, month, day) = sdn_to_french(jd);
    Ok(Value::string(format!("{month}/{day}/{year}")))
}

fn valid_calendar(name: &str, argument: usize, value: i64) -> Result<i64, BuiltinError> {
    if (CAL_GREGORIAN..CAL_NUM_CALS).contains(&value) {
        Ok(value)
    } else {
        Err(argument_value_error(
            name,
            &calendar_arg_label(name, argument),
            "must be a valid calendar ID",
        ))
    }
}

fn calendar_arg(name: &str, argument: usize, value: &Value) -> Result<i64, BuiltinError> {
    valid_calendar(name, argument, int_arg(name, value)?)
}

fn bounded_i32_arg(name: &str, argument: usize, value: &Value) -> Result<i64, BuiltinError> {
    let value = int_arg(name, value)?;
    if value < i32::MIN as i64 || value > i32::MAX as i64 {
        return Err(argument_value_error(
            name,
            &calendar_arg_label(name, argument),
            "must be between -2147483648 and 2147483647",
        ));
    }
    Ok(value)
}

fn bounded_positive_i32_arg(
    name: &str,
    argument: usize,
    value: &Value,
) -> Result<i64, BuiltinError> {
    let value = int_arg(name, value)?;
    if value <= 0 || value > i32::MAX as i64 - 1 {
        return Err(argument_value_error(
            name,
            &calendar_arg_label(name, argument),
            "must be between 1 and 2147483646",
        ));
    }
    Ok(value)
}

fn bounded_less_than_i32_max_arg(
    name: &str,
    argument: usize,
    value: &Value,
) -> Result<i64, BuiltinError> {
    let value = int_arg(name, value)?;
    if value >= i32::MAX as i64 - 1 {
        return Err(argument_value_error(
            name,
            &calendar_arg_label(name, argument),
            "must be less than 2147483646",
        ));
    }
    Ok(value)
}

fn calendar_arg_label(name: &str, argument: usize) -> String {
    let argument_name = match (name, argument) {
        ("cal_days_in_month", 1) | ("cal_info", 1) | ("cal_to_jd", 1) => "calendar",
        ("cal_from_jd", 2) => "calendar",
        ("cal_days_in_month", 2) | ("cal_to_jd", 2) => "month",
        ("cal_days_in_month", 3) => "year",
        ("cal_to_jd", 3) => "day",
        ("cal_to_jd", 4) => "year",
        ("gregoriantojd" | "juliantojd" | "jewishtojd" | "frenchtojd", 1) => "month",
        ("gregoriantojd" | "juliantojd" | "jewishtojd" | "frenchtojd", 2) => "day",
        ("gregoriantojd" | "juliantojd" | "jewishtojd" | "frenchtojd", 3) => "year",
        _ => return format!("#{argument}"),
    };
    format!("#{argument} (${argument_name})")
}

fn calendar_to_sdn(cal: i64, year: i64, month: i64, day: i64) -> Result<i64, BuiltinError> {
    match cal {
        CAL_GREGORIAN => Ok(gregorian_to_sdn(year, month, day)),
        CAL_JULIAN => Ok(julian_to_sdn(year, month, day)),
        CAL_JEWISH => Ok(jewish_to_sdn(year, month, day)),
        CAL_FRENCH => Ok(french_to_sdn(year, month, day)),
        _ => unreachable!("calendar ID already validated"),
    }
}

fn calendar_from_sdn(cal: i64, jd: i64) -> Result<(i64, i64, i64), BuiltinError> {
    match cal {
        CAL_GREGORIAN => Ok(sdn_to_gregorian(jd)),
        CAL_JULIAN => Ok(sdn_to_julian(jd)),
        CAL_JEWISH => Ok(sdn_to_jewish(jd)),
        CAL_FRENCH => Ok(sdn_to_french(jd)),
        _ => unreachable!("calendar ID already validated"),
    }
}

fn month_name(cal: i64, month: i64, long: bool) -> Result<&'static str, BuiltinError> {
    let index = usize::try_from(month).unwrap_or(0);
    match cal {
        CAL_GREGORIAN | CAL_JULIAN => Ok(if long {
            MONTH_NAME_LONG.get(index).copied().unwrap_or("")
        } else {
            MONTH_NAME_SHORT.get(index).copied().unwrap_or("")
        }),
        CAL_JEWISH => Ok(jewish_month_name(1, month)),
        CAL_FRENCH => Ok(FRENCH_MONTH_NAME.get(index).copied().unwrap_or("")),
        _ => unreachable!("calendar ID already validated"),
    }
}

fn cal_info_array(cal: i64) -> PhpArray {
    let mut result = PhpArray::new();
    let (name, symbol, num_months, max_days) = match cal {
        CAL_GREGORIAN => ("Gregorian", "CAL_GREGORIAN", 12, 31),
        CAL_JULIAN => ("Julian", "CAL_JULIAN", 12, 31),
        CAL_JEWISH => ("Jewish", "CAL_JEWISH", 13, 30),
        CAL_FRENCH => ("French", "CAL_FRENCH", 13, 30),
        _ => unreachable!("calendar ID already validated"),
    };
    let mut months = PhpArray::new();
    let mut abbrev = PhpArray::new();
    for month in 1..=num_months {
        let (long, short) = match cal {
            CAL_GREGORIAN | CAL_JULIAN => (
                MONTH_NAME_LONG[month as usize],
                MONTH_NAME_SHORT[month as usize],
            ),
            CAL_FRENCH => (
                FRENCH_MONTH_NAME[month as usize],
                FRENCH_MONTH_NAME[month as usize],
            ),
            CAL_JEWISH => (
                JEWISH_MONTH_NAME_LEAP[month as usize],
                JEWISH_MONTH_NAME_LEAP[month as usize],
            ),
            _ => unreachable!(),
        };
        months.insert(ArrayKey::Int(month), Value::string(long));
        abbrev.insert(ArrayKey::Int(month), Value::string(short));
    }
    result.insert(string_key("months"), Value::Array(months));
    result.insert(string_key("abbrevmonths"), Value::Array(abbrev));
    result.insert(string_key("maxdaysinmonth"), Value::Int(max_days));
    result.insert(string_key("calname"), Value::string(name));
    result.insert(string_key("calsymbol"), Value::string(symbol));
    result
}

fn gregorian_to_sdn(input_year: i64, input_month: i64, input_day: i64) -> i64 {
    const GREGOR_SDN_OFFSET: i64 = 32_045;
    const DAYS_PER_5_MONTHS: i64 = 153;
    const DAYS_PER_4_YEARS: i64 = 1_461;
    const DAYS_PER_400_YEARS: i64 = 146_097;

    if input_year == 0
        || input_year < -4714
        || input_month <= 0
        || input_month > 12
        || input_day <= 0
        || input_day > 31
        || (input_year == -4714 && (input_month < 11 || (input_month == 11 && input_day < 25)))
    {
        return 0;
    }
    let mut year = if input_year < 0 {
        input_year + 4801
    } else {
        input_year + 4800
    };
    let month = if input_month > 2 {
        input_month - 3
    } else {
        year -= 1;
        input_month + 9
    };
    ((year / 100) * DAYS_PER_400_YEARS) / 4
        + ((year % 100) * DAYS_PER_4_YEARS) / 4
        + (month * DAYS_PER_5_MONTHS + 2) / 5
        + input_day
        - GREGOR_SDN_OFFSET
}

fn sdn_to_gregorian(sdn: i64) -> (i64, i64, i64) {
    const GREGOR_SDN_OFFSET: i64 = 32_045;
    const DAYS_PER_5_MONTHS: i64 = 153;
    const DAYS_PER_4_YEARS: i64 = 1_461;
    const DAYS_PER_400_YEARS: i64 = 146_097;

    if sdn <= 0 {
        return (0, 0, 0);
    }
    if sdn > (i64::MAX - 4 * GREGOR_SDN_OFFSET) / 4 {
        return (0, 0, 0);
    }
    let mut temp = (sdn + GREGOR_SDN_OFFSET) * 4 - 1;
    if temp < 0 || temp / DAYS_PER_400_YEARS > i32::MAX as i64 {
        return (0, 0, 0);
    }
    let century = temp / DAYS_PER_400_YEARS;
    temp = ((temp % DAYS_PER_400_YEARS) / 4) * 4 + 3;
    if century > (i32::MAX as i64 / 100) - (temp / DAYS_PER_4_YEARS) {
        return (0, 0, 0);
    }
    let mut year = (century * 100) + (temp / DAYS_PER_4_YEARS);
    let day_of_year = (temp % DAYS_PER_4_YEARS) / 4 + 1;
    temp = day_of_year * 5 - 3;
    let mut month = temp / DAYS_PER_5_MONTHS;
    let day = (temp % DAYS_PER_5_MONTHS) / 5 + 1;
    if month < 10 {
        month += 3;
    } else {
        year += 1;
        month -= 9;
    }
    year -= 4800;
    if year <= 0 {
        year -= 1;
    }
    (year, month, day)
}

fn julian_to_sdn(input_year: i64, input_month: i64, input_day: i64) -> i64 {
    const JULIAN_SDN_OFFSET: i64 = 32_083;
    const DAYS_PER_5_MONTHS: i64 = 153;
    const DAYS_PER_4_YEARS: i64 = 1_461;

    if input_year == 0
        || input_year < -4713
        || input_month <= 0
        || input_month > 12
        || input_day <= 0
        || input_day > 31
        || (input_year == -4713 && input_month == 1 && input_day == 1)
    {
        return 0;
    }
    let mut year = if input_year < 0 {
        input_year + 4801
    } else {
        input_year + 4800
    };
    let month = if input_month > 2 {
        input_month - 3
    } else {
        year -= 1;
        input_month + 9
    };
    (year * DAYS_PER_4_YEARS) / 4 + (month * DAYS_PER_5_MONTHS + 2) / 5 + input_day
        - JULIAN_SDN_OFFSET
}

fn sdn_to_julian(sdn: i64) -> (i64, i64, i64) {
    const JULIAN_SDN_OFFSET: i64 = 32_083;
    const DAYS_PER_5_MONTHS: i64 = 153;
    const DAYS_PER_4_YEARS: i64 = 1_461;

    if sdn <= 0 {
        return (0, 0, 0);
    }
    if sdn > (i64::MAX - JULIAN_SDN_OFFSET * 4 + 1) / 4 || sdn < i64::MIN / 4 {
        return (0, 0, 0);
    }
    let mut temp = sdn * 4 + (JULIAN_SDN_OFFSET * 4 - 1);
    let mut year = temp / DAYS_PER_4_YEARS;
    if year > i32::MAX as i64 || year < i32::MIN as i64 {
        return (0, 0, 0);
    }
    let day_of_year = (temp % DAYS_PER_4_YEARS) / 4 + 1;
    temp = day_of_year * 5 - 3;
    let mut month = temp / DAYS_PER_5_MONTHS;
    let day = (temp % DAYS_PER_5_MONTHS) / 5 + 1;
    if month < 10 {
        month += 3;
    } else {
        year += 1;
        month -= 9;
    }
    year -= 4800;
    if year <= 0 {
        year -= 1;
    }
    (year, month, day)
}

fn french_to_sdn(year: i64, month: i64, day: i64) -> i64 {
    if !(1..=14).contains(&year) || !(1..=13).contains(&month) || !(1..=30).contains(&day) {
        return 0;
    }
    (year * DAYS_PER_4_YEARS) / 4 + (month - 1) * DAYS_PER_FRENCH_MONTH + day + FRENCH_SDN_OFFSET
}

fn sdn_to_french(sdn: i64) -> (i64, i64, i64) {
    if !(FRENCH_FIRST_VALID..=FRENCH_LAST_VALID).contains(&sdn) {
        return (0, 0, 0);
    }
    let temp = (sdn - FRENCH_SDN_OFFSET) * 4 - 1;
    let year = temp / DAYS_PER_4_YEARS;
    let day_of_year = (temp % DAYS_PER_4_YEARS) / 4;
    let month = day_of_year / DAYS_PER_FRENCH_MONTH + 1;
    let day = day_of_year % DAYS_PER_FRENCH_MONTH + 1;
    (year, month, day)
}

fn jewish_to_sdn(year: i64, month: i64, day: i64) -> i64 {
    if year <= 0 || year >= i32::MAX as i64 - 1 || day <= 0 || day > 30 {
        return 0;
    }
    let sdn = match month {
        1 | 2 => {
            let (_, _, _, _, tishri1) = find_start_of_jewish_year(year);
            if month == 1 {
                tishri1 + day - 1
            } else {
                tishri1 + day + 29
            }
        }
        3 => {
            let (_, metonic_year, mut molad_day, mut molad_halakim, tishri1) =
                find_start_of_jewish_year(year);
            molad_halakim += HALAKIM_PER_LUNAR_CYCLE * JEWISH_MONTHS_PER_YEAR[metonic_year];
            molad_day += molad_halakim / HALAKIM_PER_DAY;
            molad_halakim %= HALAKIM_PER_DAY;
            let tishri1_after = jewish_tishri1(
                ((metonic_year as i64 + 1) % 19) as usize,
                molad_day,
                molad_halakim,
            );
            let year_length = tishri1_after - tishri1;
            if year_length == 355 || year_length == 385 {
                tishri1 + day + 59
            } else {
                tishri1 + day + 58
            }
        }
        4..=6 => {
            let (_, _, _, _, tishri1_after) = find_start_of_jewish_year(year + 1);
            let length_of_adar = if jewish_months_in_year(year) == 12 {
                29
            } else {
                59
            };
            if month == 4 {
                tishri1_after + day - length_of_adar - 237
            } else if month == 5 {
                tishri1_after + day - length_of_adar - 208
            } else {
                tishri1_after + day - length_of_adar - 178
            }
        }
        7..=13 => {
            let (_, _, _, _, tishri1_after) = find_start_of_jewish_year(year + 1);
            match month {
                7 => tishri1_after + day - 207,
                8 => tishri1_after + day - 178,
                9 => tishri1_after + day - 148,
                10 => tishri1_after + day - 119,
                11 => tishri1_after + day - 89,
                12 => tishri1_after + day - 60,
                13 => tishri1_after + day - 30,
                _ => unreachable!(),
            }
        }
        _ => return 0,
    };
    sdn + JEWISH_SDN_OFFSET
}

fn sdn_to_jewish(sdn: i64) -> (i64, i64, i64) {
    if sdn <= JEWISH_SDN_OFFSET || sdn > JEWISH_SDN_MAX {
        return (0, 0, 0);
    }

    let input_day = sdn - JEWISH_SDN_OFFSET;
    let (mut metonic_cycle, mut metonic_year, mut day, mut halakim) = find_tishri_molad(input_day);
    let mut tishri1 = jewish_tishri1(metonic_year, day, halakim);

    if input_day >= tishri1 {
        let year = metonic_cycle * 19 + metonic_year as i64 + 1;
        if input_day < tishri1 + 59 {
            if input_day < tishri1 + 30 {
                return (year, 1, input_day - tishri1 + 1);
            }
            return (year, 2, input_day - tishri1 - 29);
        }
        halakim += HALAKIM_PER_LUNAR_CYCLE * JEWISH_MONTHS_PER_YEAR[metonic_year];
        day += halakim / HALAKIM_PER_DAY;
        halakim %= HALAKIM_PER_DAY;
        let tishri1_after = jewish_tishri1((metonic_year + 1) % 19, day, halakim);
        return jewish_ambiguous_month_from_year_length(input_day, year, tishri1, tishri1_after);
    }

    let year = metonic_cycle * 19 + metonic_year as i64;
    if input_day >= tishri1 - 177 {
        if input_day > tishri1 - 30 {
            return (year, 13, input_day - tishri1 + 30);
        } else if input_day > tishri1 - 60 {
            return (year, 12, input_day - tishri1 + 60);
        } else if input_day > tishri1 - 89 {
            return (year, 11, input_day - tishri1 + 89);
        } else if input_day > tishri1 - 119 {
            return (year, 10, input_day - tishri1 + 119);
        } else if input_day > tishri1 - 148 {
            return (year, 9, input_day - tishri1 + 148);
        }
        return (year, 8, input_day - tishri1 + 178);
    }

    let mut month = 7;
    let mut date_day = input_day - tishri1 + 207;
    if jewish_months_in_year(year) == 13 {
        if date_day > 0 {
            return (year, month, date_day);
        }
        month -= 1;
        date_day += 30;
        if date_day > 0 {
            return (year, month, date_day);
        }
        month -= 1;
        date_day += 30;
    } else {
        if date_day > 0 {
            return (year, month, date_day);
        }
        month -= 2;
        date_day += 30;
    }
    if date_day > 0 {
        return (year, month, date_day);
    }
    month -= 1;
    date_day += 29;
    if date_day > 0 {
        return (year, month, date_day);
    }

    let tishri1_after = tishri1;
    (metonic_cycle, metonic_year, day, halakim) = find_tishri_molad(day - 365);
    let _ = metonic_cycle;
    tishri1 = jewish_tishri1(metonic_year, day, halakim);
    jewish_ambiguous_month_from_year_length(input_day, year, tishri1, tishri1_after)
}

fn jewish_ambiguous_month_from_year_length(
    input_day: i64,
    year: i64,
    tishri1: i64,
    tishri1_after: i64,
) -> (i64, i64, i64) {
    let year_length = tishri1_after - tishri1;
    let mut day = input_day - tishri1 - 29;
    if year_length == 355 || year_length == 385 {
        if day <= 30 {
            return (year, 2, day);
        }
        day -= 30;
    } else {
        if day <= 29 {
            return (year, 2, day);
        }
        day -= 29;
    }
    (year, 3, day)
}

fn find_start_of_jewish_year(year: i64) -> (i64, usize, i64, i64, i64) {
    let metonic_cycle = (year - 1) / 19;
    let metonic_year = ((year - 1) % 19) as usize;
    let (mut molad_day, mut molad_halakim) = molad_of_metonic_cycle(metonic_cycle);
    molad_halakim += HALAKIM_PER_LUNAR_CYCLE * JEWISH_YEAR_OFFSET[metonic_year];
    molad_day += molad_halakim / HALAKIM_PER_DAY;
    molad_halakim %= HALAKIM_PER_DAY;
    let tishri1 = jewish_tishri1(metonic_year, molad_day, molad_halakim);
    (
        metonic_cycle,
        metonic_year,
        molad_day,
        molad_halakim,
        tishri1,
    )
}

fn find_tishri_molad(input_day: i64) -> (i64, usize, i64, i64) {
    let mut metonic_cycle = (input_day + 310) / 6_940;
    let (mut molad_day, mut molad_halakim) = molad_of_metonic_cycle(metonic_cycle);
    while molad_day < input_day - 6_940 + 310 {
        metonic_cycle += 1;
        molad_halakim += HALAKIM_PER_METONIC_CYCLE;
        molad_day += molad_halakim / HALAKIM_PER_DAY;
        molad_halakim %= HALAKIM_PER_DAY;
    }

    let mut metonic_year = 0usize;
    while metonic_year < 18 {
        if molad_day > input_day - 74 {
            break;
        }
        molad_halakim += HALAKIM_PER_LUNAR_CYCLE * JEWISH_MONTHS_PER_YEAR[metonic_year];
        molad_day += molad_halakim / HALAKIM_PER_DAY;
        molad_halakim %= HALAKIM_PER_DAY;
        metonic_year += 1;
    }
    (metonic_cycle, metonic_year, molad_day, molad_halakim)
}

fn molad_of_metonic_cycle(metonic_cycle: i64) -> (i64, i64) {
    let total_halakim =
        NEW_MOON_OF_CREATION as i128 + metonic_cycle as i128 * HALAKIM_PER_METONIC_CYCLE as i128;
    (
        (total_halakim / HALAKIM_PER_DAY as i128) as i64,
        (total_halakim % HALAKIM_PER_DAY as i128) as i64,
    )
}

fn jewish_tishri1(metonic_year: usize, molad_day: i64, molad_halakim: i64) -> i64 {
    let mut tishri1 = molad_day;
    let mut dow = tishri1 % 7;
    let leap_year = matches!(metonic_year, 2 | 5 | 7 | 10 | 13 | 16 | 18);
    let last_was_leap_year = matches!(metonic_year, 0 | 3 | 6 | 8 | 11 | 14 | 17);

    if molad_halakim >= NOON
        || (!leap_year && dow == TUESDAY && molad_halakim >= AM3_11_20)
        || (last_was_leap_year && dow == MONDAY && molad_halakim >= AM9_32_43)
    {
        tishri1 += 1;
        dow += 1;
        if dow == 7 {
            dow = 0;
        }
    }
    if dow == WEDNESDAY || dow == FRIDAY || dow == SUNDAY {
        tishri1 += 1;
    }
    tishri1
}

fn jewish_months_in_year(year: i64) -> i64 {
    JEWISH_MONTHS_PER_YEAR[((year - 1) % 19) as usize]
}

fn jewish_month_name(year: i64, month: i64) -> &'static str {
    let Ok(index) = usize::try_from(month) else {
        return "";
    };
    if year > 0 && jewish_months_in_year(year) == 12 {
        JEWISH_MONTH_NAME.get(index).copied().unwrap_or("")
    } else {
        JEWISH_MONTH_NAME_LEAP.get(index).copied().unwrap_or("")
    }
}

fn jewish_hebrew_month_name(year: i64, month: i64) -> &'static [u8] {
    let Ok(index) = usize::try_from(month) else {
        return b"";
    };
    if year > 0 && jewish_months_in_year(year) == 12 {
        JEWISH_HEB_MONTH_NAME.get(index).copied().unwrap_or(b"")
    } else {
        JEWISH_HEB_MONTH_NAME_LEAP
            .get(index)
            .copied()
            .unwrap_or(b"")
    }
}

fn hebrew_number_to_chars(n: i64, flags: i64) -> Option<Vec<u8>> {
    if !(1..=9999).contains(&n) {
        return None;
    }

    let mut n = n;
    let mut out = Vec::with_capacity(18);
    let mut end_of_alafim = 0usize;

    if n / 1000 > 0 {
        out.push(HEBREW_NUMBER_LETTERS[(n / 1000) as usize]);
        if flags & CAL_JEWISH_ADD_ALAFIM_GERESH != 0 {
            out.push(b'\'');
        }
        if flags & CAL_JEWISH_ADD_ALAFIM != 0 {
            out.extend_from_slice(b" \xE0\xEC\xF4\xE9\xED ");
        }
        end_of_alafim = out.len();
        n %= 1000;
    }

    while n >= 400 {
        out.push(HEBREW_NUMBER_LETTERS[22]);
        n -= 400;
    }
    if n >= 100 {
        out.push(HEBREW_NUMBER_LETTERS[(18 + n / 100) as usize]);
        n %= 100;
    }
    if n == 15 || n == 16 {
        out.push(HEBREW_NUMBER_LETTERS[9]);
        out.push(HEBREW_NUMBER_LETTERS[(n - 9) as usize]);
    } else {
        if n >= 10 {
            out.push(HEBREW_NUMBER_LETTERS[(9 + n / 10) as usize]);
            n %= 10;
        }
        if n > 0 {
            out.push(HEBREW_NUMBER_LETTERS[n as usize]);
        }
    }

    if flags & CAL_JEWISH_ADD_GERESHAYIM != 0 {
        match out.len().saturating_sub(end_of_alafim) {
            0 => {}
            1 => out.push(b'\''),
            _ => out.insert(out.len() - 1, b'"'),
        }
    }

    Some(out)
}

fn month_name_by_index(names: &'static [&'static str], month: i64) -> &'static str {
    let Ok(index) = usize::try_from(month) else {
        return "";
    };
    names.get(index).copied().unwrap_or("")
}

fn day_of_week(sdn: i64) -> i64 {
    (sdn % 7 + 8) % 7
}

fn easter_days(year: i64, method: i64) -> i64 {
    let golden = (year % 19) + 1;
    let (dom, mut pfm) = if (year <= 1582 && method != CAL_EASTER_ALWAYS_GREGORIAN)
        || ((1583..=1752).contains(&year)
            && method != CAL_EASTER_ROMAN
            && method != CAL_EASTER_ALWAYS_GREGORIAN)
        || method == CAL_EASTER_ALWAYS_JULIAN
    {
        (
            positive_mod(year + (year / 4) + 5, 7),
            positive_mod(3 - (11 * golden) - 7, 30),
        )
    } else {
        let solar = (year - 1600) / 100 - (year - 1600) / 400;
        let lunar = (((year - 1400) / 100) * 8) / 25;
        (
            positive_mod(year + (year / 4) - (year / 100) + (year / 400), 7),
            positive_mod(3 - (11 * golden) + solar - lunar, 30),
        )
    };
    if pfm == 29 || (pfm == 28 && golden > 11) {
        pfm -= 1;
    }
    let tmp = positive_mod(4 - pfm - dom, 7);
    pfm + tmp + 1
}

fn validate_easter_year(name: &str, year: i64, timestamp: bool) -> Result<(), BuiltinError> {
    if year <= 0 || year > EASTER_MAX_YEAR {
        return Err(argument_value_error(
            name,
            "#1 ($year)",
            &format!("must be between 1 and {EASTER_MAX_YEAR}"),
        ));
    }
    if timestamp && year < 1970 {
        return Err(argument_value_error(
            name,
            "#1 ($year)",
            "must be a year after 1970 (inclusive)",
        ));
    }
    if timestamp && year > 2_000_000_000 {
        return Err(argument_value_error(
            name,
            "#1 ($year)",
            "must be a year before 2.000.000.000 (inclusive)",
        ));
    }
    Ok(())
}

fn positive_mod(value: i64, modulus: i64) -> i64 {
    let value = value % modulus;
    if value < 0 { value + modulus } else { value }
}

fn string_key(value: &str) -> ArrayKey {
    ArrayKey::String(PhpString::from_test_str(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OutputBuffer;

    fn call(name: &str, args: Vec<Value>) -> Value {
        call_result(name, args).unwrap()
    }

    fn call_result(name: &str, args: Vec<Value>) -> BuiltinResult {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        ENTRIES
            .iter()
            .find(|entry| entry.name() == name)
            .unwrap()
            .function()(&mut context, args, RuntimeSourceSpan::default())
    }

    fn array_int(array: &PhpArray, key: &str) -> i64 {
        let Some(Value::Int(value)) = array.get(&string_key(key)) else {
            panic!("expected integer key {key}");
        };
        *value
    }

    fn string_bytes(value: Value) -> Vec<u8> {
        let Value::String(value) = value else {
            panic!("expected string value");
        };
        value.as_bytes().to_vec()
    }

    #[test]
    fn gregorian_and_julian_conversions_match_php_src_examples() {
        assert_eq!(
            call(
                "gregoriantojd",
                vec![Value::Int(7), Value::Int(4), Value::Int(2026)]
            ),
            Value::Int(2_461_226)
        );
        assert_eq!(
            call("jdtogregorian", vec![Value::Int(2_461_226)]),
            Value::string("7/4/2026")
        );
        assert_eq!(
            call(
                "juliantojd",
                vec![Value::Int(6), Value::Int(21), Value::Int(2026)]
            ),
            Value::Int(2_461_226)
        );
        assert_eq!(
            call("jdtojulian", vec![Value::Int(2_461_226)]),
            Value::string("6/21/2026")
        );
        assert_eq!(
            call(
                "juliantojd",
                vec![Value::Int(5), Value::Int(5), Value::Int(6_000_000_000)]
            ),
            Value::Int(622_764_916_319)
        );
    }

    #[test]
    fn calendar_names_days_and_unix_helpers_cover_supported_slice() {
        assert_eq!(
            call(
                "cal_days_in_month",
                vec![Value::Int(CAL_GREGORIAN), Value::Int(2), Value::Int(2024)]
            ),
            Value::Int(29)
        );
        assert_eq!(
            call(
                "jddayofweek",
                vec![Value::Int(2_461_226), Value::Int(CAL_DOW_LONG)]
            ),
            Value::string("Saturday")
        );
        assert_eq!(
            call(
                "jdmonthname",
                vec![Value::Int(2_461_226), Value::Int(CAL_MONTH_GREGORIAN_LONG)]
            ),
            Value::string("July")
        );
        assert_eq!(
            call("jdtounix", vec![Value::Int(UNIX_EPOCH_SDN)]),
            Value::Int(0)
        );
        assert_eq!(
            call("unixtojd", vec![Value::Int(0)]),
            Value::Int(UNIX_EPOCH_SDN)
        );
    }

    #[test]
    fn french_final_month_and_unix_bounds_match_php_src() {
        assert_eq!(
            call(
                "cal_days_in_month",
                vec![Value::Int(CAL_FRENCH), Value::Int(13), Value::Int(14)]
            ),
            Value::Int(5)
        );
        assert_eq!(
            call("jdtounix", vec![Value::Int(UNIX_MAX_JD)]),
            Value::Int(9_223_372_036_854_720_000)
        );
        let error = call_result("jdtounix", vec![Value::Int(UNIX_MAX_JD + 1)])
            .expect_err("out-of-range Julian day should fail");
        assert!(format!("{error:?}").contains("jday must be between 2440588 and 106751993607888"));
    }

    #[test]
    fn easter_days_uses_php_src_algorithm() {
        assert_eq!(call("easter_days", vec![Value::Int(2026)]), Value::Int(15));
        assert_eq!(
            call(
                "easter_days",
                vec![Value::Int(1752), Value::Int(CAL_EASTER_ALWAYS_GREGORIAN)]
            ),
            Value::Int(12)
        );
    }

    #[test]
    fn calendar_extreme_serial_days_do_not_overflow() {
        let Value::Array(julian) = call(
            "cal_from_jd",
            vec![
                Value::Int(3_315_881_921_229_094_912),
                Value::Int(CAL_JULIAN),
            ],
        ) else {
            panic!("expected array");
        };
        assert_eq!(array_int(&julian, "month"), 0);
        assert_eq!(array_int(&julian, "day"), 0);
        assert_eq!(array_int(&julian, "year"), 0);
        assert_eq!(array_int(&julian, "dow"), 3);

        let Value::Array(gregorian) = call(
            "cal_from_jd",
            vec![
                Value::Int(9_223_372_036_854_743_639),
                Value::Int(CAL_GREGORIAN),
            ],
        ) else {
            panic!("expected array");
        };
        assert_eq!(array_int(&gregorian, "month"), 0);
        assert_eq!(array_int(&gregorian, "day"), 0);
        assert_eq!(array_int(&gregorian, "year"), 0);
    }

    #[test]
    fn easter_year_bounds_reject_overflow_inputs() {
        let error = call_result(
            "easter_days",
            vec![Value::Int(i64::MAX), Value::Int(CAL_EASTER_DEFAULT)],
        )
        .expect_err("large Easter year should fail");
        assert!(format!("{error:?}").contains("must be between 1 and"));

        let too_early = call_result(
            "easter_date",
            vec![Value::Int(1969), Value::Int(CAL_EASTER_DEFAULT)],
        )
        .expect_err("pre-epoch Easter year should fail");
        assert!(format!("{too_early:?}").contains("must be a year after 1970"));

        let too_late = call_result(
            "easter_date",
            vec![Value::Int(2_000_000_001), Value::Int(CAL_EASTER_DEFAULT)],
        )
        .expect_err("large timestamp Easter year should fail");
        assert!(format!("{too_late:?}").contains("must be a year before 2.000.000.000"));
    }

    #[test]
    fn jewish_and_french_conversions_match_php_src_fixtures() {
        assert_eq!(
            call(
                "jewishtojd",
                vec![Value::Int(2), Value::Int(22), Value::Int(5763)]
            ),
            Value::Int(2_452_576)
        );
        assert_eq!(
            call("jdtojewish", vec![Value::Int(2_452_576)]),
            Value::string("2/22/5763")
        );
        assert_eq!(
            call(
                "frenchtojd",
                vec![Value::Int(1), Value::Int(1), Value::Int(1)]
            ),
            Value::Int(2_375_840)
        );
        assert_eq!(
            call("jdtofrench", vec![Value::Int(2_375_940)]),
            Value::string("4/11/1")
        );
        assert_eq!(
            call(
                "cal_to_jd",
                vec![
                    Value::Int(CAL_FRENCH),
                    Value::Int(1),
                    Value::Int(1),
                    Value::Int(1)
                ]
            ),
            Value::Int(2_375_840)
        );
        assert_eq!(
            call(
                "jdmonthname",
                vec![Value::Int(2_453_396), Value::Int(CAL_MONTH_JEWISH)]
            ),
            Value::string("Shevat")
        );
    }

    #[test]
    fn jdtojewish_hebrew_formatting_matches_php_src_bytes() {
        let jd = call(
            "jewishtojd",
            vec![Value::Int(1), Value::Int(1), Value::Int(5000)],
        );
        assert_eq!(
            string_bytes(call("jdtojewish", vec![jd, Value::Bool(true)])),
            b"\xE0 \xFA\xF9\xF8\xE9 \xE4".to_vec()
        );

        assert_eq!(
            string_bytes(call(
                "jdtojewish",
                vec![Value::Int(2_452_576), Value::Bool(true), Value::Int(14)]
            )),
            b"\xEB\"\xE1 \xE7\xF9\xE5\xEF \xE4' \xE0\xEC\xF4\xE9\xED \xFA\xF9\xF1\"\xE2".to_vec()
        );
    }
}

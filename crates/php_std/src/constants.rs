//! PHP 8.5.7 core and platform constants for standard-library.

use php_runtime::api::{FloatValue, PhpString, Value};

use crate::ConstantValue;

/// Target PHP version.
pub const PHP_VERSION: &str = "8.5.7";
pub const ZEND_VERSION: &str = "4.5.7";
/// Target PHP version ID.
pub const PHP_VERSION_ID: i64 = 80507;
/// Target PHP major version.
pub const PHP_MAJOR_VERSION: i64 = 8;
/// Target PHP minor version.
pub const PHP_MINOR_VERSION: i64 = 5;
/// Target PHP release version.
pub const PHP_RELEASE_VERSION: i64 = 7;
/// PHP integer size in bytes for the current build target.
pub const PHP_INT_SIZE: i64 = std::mem::size_of::<isize>() as i64;
/// PHP maximum integer for the current build target.
pub const PHP_INT_MAX: i64 = isize::MAX as i64;
/// PHP minimum integer for the current build target.
pub const PHP_INT_MIN: i64 = isize::MIN as i64;
/// Number of decimal digits that can be rounded into a float and back without precision loss.
pub const PHP_FLOAT_DIG: i64 = f64::DIGITS as i64;
/// Difference between 1.0 and the next representable float.
pub const PHP_FLOAT_EPSILON: FloatValue = FloatValue::from_f64(f64::EPSILON);
/// Maximum finite PHP float.
pub const PHP_FLOAT_MAX: FloatValue = FloatValue::from_f64(f64::MAX);
/// Minimum positive normalized PHP float.
pub const PHP_FLOAT_MIN: FloatValue = FloatValue::from_f64(f64::MIN_POSITIVE);
/// PHP positive infinity constant.
pub const INF: FloatValue = FloatValue::from_f64(f64::INFINITY);
/// PHP quiet NaN constant.
pub const NAN: FloatValue = FloatValue::from_f64(f64::NAN);
/// Euler's number.
pub const M_E: FloatValue = FloatValue::from_f64(std::f64::consts::E);
/// Base-2 logarithm of Euler's number.
pub const M_LOG2E: FloatValue = FloatValue::from_f64(std::f64::consts::LOG2_E);
/// Base-10 logarithm of Euler's number.
pub const M_LOG10E: FloatValue = FloatValue::from_f64(std::f64::consts::LOG10_E);
/// Natural logarithm of 2.
pub const M_LN2: FloatValue = FloatValue::from_f64(std::f64::consts::LN_2);
/// Natural logarithm of 10.
pub const M_LN10: FloatValue = FloatValue::from_f64(std::f64::consts::LN_10);
/// Pi.
pub const M_PI: FloatValue = FloatValue::from_f64(std::f64::consts::PI);
/// Pi divided by 2.
pub const M_PI_2: FloatValue = FloatValue::from_f64(std::f64::consts::FRAC_PI_2);
/// Pi divided by 4.
pub const M_PI_4: FloatValue = FloatValue::from_f64(std::f64::consts::FRAC_PI_4);
/// Reciprocal of pi.
pub const M_1_PI: FloatValue = FloatValue::from_f64(std::f64::consts::FRAC_1_PI);
/// 2 divided by pi.
pub const M_2_PI: FloatValue = FloatValue::from_f64(std::f64::consts::FRAC_2_PI);
/// Square root of pi.
pub const M_SQRTPI: FloatValue = FloatValue::from_f64(1.772_453_850_905_516);
/// 2 divided by the square root of pi.
pub const M_2_SQRTPI: FloatValue = FloatValue::from_f64(std::f64::consts::FRAC_2_SQRT_PI);
/// Natural logarithm of pi.
pub const M_LNPI: FloatValue = FloatValue::from_f64(1.144_729_885_849_400_2);
/// Euler-Mascheroni constant.
pub const M_EULER: FloatValue = FloatValue::from_f64(0.577_215_664_901_532_9);
/// Square root of 2.
pub const M_SQRT2: FloatValue = FloatValue::from_f64(std::f64::consts::SQRT_2);
/// Reciprocal of the square root of 2.
pub const M_SQRT1_2: FloatValue = FloatValue::from_f64(std::f64::consts::FRAC_1_SQRT_2);
/// Square root of 3.
pub const M_SQRT3: FloatValue = FloatValue::from_f64(1.732_050_807_568_877_2);

/// Gregorian calendar ID.
pub const CAL_GREGORIAN: i64 = 0;
/// Julian calendar ID.
pub const CAL_JULIAN: i64 = 1;
/// Jewish calendar ID.
pub const CAL_JEWISH: i64 = 2;
/// French republican calendar ID.
pub const CAL_FRENCH: i64 = 3;
/// Number of calendar IDs.
pub const CAL_NUM_CALS: i64 = 4;
/// Return numeric day of week from `jddayofweek`.
pub const CAL_DOW_DAYNO: i64 = 0;
/// Return long day name from `jddayofweek`.
pub const CAL_DOW_LONG: i64 = 1;
/// Return abbreviated day name from `jddayofweek`.
pub const CAL_DOW_SHORT: i64 = 2;
/// Return abbreviated Gregorian month name from `jdmonthname`.
pub const CAL_MONTH_GREGORIAN_SHORT: i64 = 0;
/// Return long Gregorian month name from `jdmonthname`.
pub const CAL_MONTH_GREGORIAN_LONG: i64 = 1;
/// Return abbreviated Julian month name from `jdmonthname`.
pub const CAL_MONTH_JULIAN_SHORT: i64 = 2;
/// Return long Julian month name from `jdmonthname`.
pub const CAL_MONTH_JULIAN_LONG: i64 = 3;
/// Return Jewish month name from `jdmonthname`.
pub const CAL_MONTH_JEWISH: i64 = 4;
/// Return French republican month name from `jdmonthname`.
pub const CAL_MONTH_FRENCH: i64 = 5;
/// Default Easter calculation mode.
pub const CAL_EASTER_DEFAULT: i64 = 0;
/// Roman Easter calculation mode.
pub const CAL_EASTER_ROMAN: i64 = 1;
/// Always use Gregorian Easter calculation mode.
pub const CAL_EASTER_ALWAYS_GREGORIAN: i64 = 2;
/// Always use Julian Easter calculation mode.
pub const CAL_EASTER_ALWAYS_JULIAN: i64 = 3;
/// Hebrew formatting flag.
pub const CAL_JEWISH_ADD_ALAFIM_GERESH: i64 = 2;
/// Hebrew formatting flag.
pub const CAL_JEWISH_ADD_ALAFIM: i64 = 4;
/// Hebrew formatting flag.
pub const CAL_JEWISH_ADD_GERESHAYIM: i64 = 8;

/// Round halves away from zero.
pub const PHP_ROUND_HALF_UP: i64 = 1;
/// Round halves toward zero.
pub const PHP_ROUND_HALF_DOWN: i64 = 2;
/// Round halves to the nearest even integer.
pub const PHP_ROUND_HALF_EVEN: i64 = 3;
/// Round halves to the nearest odd integer.
pub const PHP_ROUND_HALF_ODD: i64 = 4;

/// Directory separator for the current build target.
#[cfg(windows)]
pub const DIRECTORY_SEPARATOR: &str = "\\";
/// Directory separator for the current build target.
#[cfg(not(windows))]
pub const DIRECTORY_SEPARATOR: &str = "/";

/// Path separator for the current build target.
#[cfg(windows)]
pub const PATH_SEPARATOR: &str = ";";
/// Path separator for the current build target.
#[cfg(not(windows))]
pub const PATH_SEPARATOR: &str = ":";

/// PHP end-of-line constant for this CLI engine.
pub const PHP_EOL: &str = "\n";
/// PHP server API for this CLI engine.
pub const PHP_SAPI: &str = "cli";
/// PHP binary display path for this CLI engine.
pub const PHP_BINARY: &str = "phrust-php";
/// PHP extension directory display path for this compatibility surface.
pub const PHP_EXTENSION_DIR: &str = "/usr/local/lib/php/extensions/debug-non-zts-20250925";
/// PEAR extension directory display path for this compatibility surface.
pub const PEAR_EXTENSION_DIR: &str = PHP_EXTENSION_DIR;
/// PEAR install directory display path for this compatibility surface.
pub const PEAR_INSTALL_DIR: &str = "";
/// PHP binary directory display path for this compatibility surface.
pub const PHP_BINDIR: &str = "/usr/local/bin";
/// PHP build date display string for this compatibility surface.
pub const PHP_BUILD_DATE: &str = "Jan  1 1980 00:00:00";
/// Whether CLI process-title support is exposed.
pub const PHP_CLI_PROCESS_TITLE: bool = true;
/// PHP config file directory display path for this compatibility surface.
pub const PHP_CONFIG_FILE_PATH: &str = "/usr/local/lib";
/// PHP config scan directory display path for this compatibility surface.
pub const PHP_CONFIG_FILE_SCAN_DIR: &str = "";
/// PHP data directory display path for this compatibility surface.
pub const PHP_DATADIR: &str = "/usr/local/share/php";
/// Whether the reference-compatible PHP surface is a debug build.
pub const PHP_DEBUG: bool = true;
/// PHP extra version suffix for this compatibility surface.
pub const PHP_EXTRA_VERSION: &str = "";
/// File descriptor set size exposed by the target PHP build.
pub const PHP_FD_SETSIZE: i64 = 1024;
/// PHP library directory display path for this compatibility surface.
pub const PHP_LIBDIR: &str = "/usr/local/lib/php";
/// PHP local state directory display path for this compatibility surface.
pub const PHP_LOCALSTATEDIR: &str = "/usr/local/var";
/// PHP manual directory display path for this compatibility surface.
pub const PHP_MANDIR: &str = "/usr/local/php/man";
/// PHP install prefix display path for this compatibility surface.
pub const PHP_PREFIX: &str = "/usr/local";
/// PHP system binary directory display path for this compatibility surface.
pub const PHP_SBINDIR: &str = "/usr/local/sbin";
/// Shared library suffix exposed by the target PHP build.
pub const PHP_SHLIB_SUFFIX: &str = "so";
/// PHP system config directory display path for this compatibility surface.
pub const PHP_SYSCONFDIR: &str = "/usr/local/etc";
/// Whether the target PHP build exposes Zend Thread Safety.
pub const PHP_ZTS: bool = false;
/// Whether the target Zend build is a debug build.
pub const ZEND_DEBUG_BUILD: bool = true;
/// Whether the target Zend build exposes thread safety.
pub const ZEND_THREAD_SAFE: bool = false;
/// Zend VM kind exposed by the target PHP build.
pub const ZEND_VM_KIND: &str = "ZEND_VM_KIND_TAILCALL";
/// Default include path for this CLI engine.
pub const DEFAULT_INCLUDE_PATH: &str = ".";
/// Maximum path length used by the compatibility surface.
pub const PHP_MAXPATHLEN: i64 = 1024;
/// Output handler phase flag for continue/write.
pub const PHP_OUTPUT_HANDLER_CONT: i64 = 0;
/// Output handler phase flag for write.
pub const PHP_OUTPUT_HANDLER_WRITE: i64 = 0;
/// Output handler phase flag for start.
pub const PHP_OUTPUT_HANDLER_START: i64 = 1;
/// Output handler phase flag for clean.
pub const PHP_OUTPUT_HANDLER_CLEAN: i64 = 2;
/// Output handler phase flag for flush.
pub const PHP_OUTPUT_HANDLER_FLUSH: i64 = 4;
/// Output handler phase flag for final/end.
pub const PHP_OUTPUT_HANDLER_FINAL: i64 = 8;
/// Output handler phase flag for end.
pub const PHP_OUTPUT_HANDLER_END: i64 = PHP_OUTPUT_HANDLER_FINAL;
/// Output handler capability flag for cleanable handlers.
pub const PHP_OUTPUT_HANDLER_CLEANABLE: i64 = 16;
/// Output handler capability flag for flushable handlers.
pub const PHP_OUTPUT_HANDLER_FLUSHABLE: i64 = 32;
/// Output handler capability flag for removable handlers.
pub const PHP_OUTPUT_HANDLER_REMOVABLE: i64 = 64;
/// Output handler default capability flags.
pub const PHP_OUTPUT_HANDLER_STDFLAGS: i64 =
    PHP_OUTPUT_HANDLER_CLEANABLE | PHP_OUTPUT_HANDLER_FLUSHABLE | PHP_OUTPUT_HANDLER_REMOVABLE;
/// Output handler state flag for started handlers.
pub const PHP_OUTPUT_HANDLER_STARTED: i64 = 4096;
/// Output handler state flag for disabled handlers.
pub const PHP_OUTPUT_HANDLER_DISABLED: i64 = 8192;
/// Output handler state flag for processed handlers.
pub const PHP_OUTPUT_HANDLER_PROCESSED: i64 = 16384;
/// Include object metadata in debug backtraces.
pub const DEBUG_BACKTRACE_PROVIDE_OBJECT: i64 = 1;
/// Omit argument values from debug backtraces.
pub const DEBUG_BACKTRACE_IGNORE_ARGS: i64 = 2;

/// PHP OS string for the current build target.
#[cfg(target_os = "macos")]
pub const PHP_OS: &str = "Darwin";
/// PHP OS string for the current build target.
#[cfg(target_os = "linux")]
pub const PHP_OS: &str = "Linux";
/// PHP OS string for the current build target.
#[cfg(target_os = "windows")]
pub const PHP_OS: &str = "WINNT";
/// PHP OS string for other targets.
#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub const PHP_OS: &str = std::env::consts::OS;

/// PHP OS family string for the current build target.
#[cfg(target_os = "macos")]
pub const PHP_OS_FAMILY: &str = "Darwin";
/// PHP OS family string for the current build target.
#[cfg(target_os = "linux")]
pub const PHP_OS_FAMILY: &str = "Linux";
/// PHP OS family string for the current build target.
#[cfg(target_os = "windows")]
pub const PHP_OS_FAMILY: &str = "Windows";
/// PHP OS family string for other targets.
#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub const PHP_OS_FAMILY: &str = "Unknown";

/// PHP `E_ERROR`.
pub const E_ERROR: i64 = 1;
/// PHP `E_WARNING`.
pub const E_WARNING: i64 = 2;
/// PHP `E_PARSE`.
pub const E_PARSE: i64 = 4;
/// PHP `E_NOTICE`.
pub const E_NOTICE: i64 = 8;
/// PHP `E_CORE_ERROR`.
pub const E_CORE_ERROR: i64 = 16;
/// PHP `E_CORE_WARNING`.
pub const E_CORE_WARNING: i64 = 32;
/// PHP `E_COMPILE_ERROR`.
pub const E_COMPILE_ERROR: i64 = 64;
/// PHP `E_COMPILE_WARNING`.
pub const E_COMPILE_WARNING: i64 = 128;
/// PHP `E_USER_ERROR`.
pub const E_USER_ERROR: i64 = 256;
/// PHP `E_USER_WARNING`.
pub const E_USER_WARNING: i64 = 512;
/// PHP `E_USER_NOTICE`.
pub const E_USER_NOTICE: i64 = 1024;
/// PHP `E_STRICT`.
pub const E_STRICT: i64 = 2048;
/// PHP `E_RECOVERABLE_ERROR`.
pub const E_RECOVERABLE_ERROR: i64 = 4096;
/// PHP `E_DEPRECATED`.
pub const E_DEPRECATED: i64 = 8192;
/// PHP `E_USER_DEPRECATED`.
pub const E_USER_DEPRECATED: i64 = 16384;
/// PHP 8.5 `E_ALL`.
pub const E_ALL: i64 = 30719;

/// Append mode flag for file writes.
pub const FILE_APPEND: i64 = 8;
/// Search include_path for file reads.
pub const FILE_USE_INCLUDE_PATH: i64 = 1;
/// Strip newlines while reading files.
pub const FILE_IGNORE_NEW_LINES: i64 = 2;
/// Skip empty lines while reading files.
pub const FILE_SKIP_EMPTY_LINES: i64 = 4;
/// Disable default stream context for file operations.
pub const FILE_NO_DEFAULT_CONTEXT: i64 = 16;
/// Shared file lock flag.
pub const LOCK_SH: i64 = 1;
/// Exclusive lock flag for file writes.
pub const LOCK_EX: i64 = 2;
/// Unlock file lock flag.
pub const LOCK_UN: i64 = 3;
/// Non-blocking lock flag.
pub const LOCK_NB: i64 = 4;
/// Seek from start of file.
pub const SEEK_SET: i64 = 0;
/// Seek from current file position.
pub const SEEK_CUR: i64 = 1;
/// Seek from end of file.
pub const SEEK_END: i64 = 2;
/// `glob()` brace expansion flag.
pub const GLOB_BRACE: i64 = 128;
/// `glob()` mark directory flag.
pub const GLOB_MARK: i64 = 8;
/// `glob()` no-sort flag.
pub const GLOB_NOSORT: i64 = 32;
/// `glob()` no-check flag.
pub const GLOB_NOCHECK: i64 = 16;
/// `glob()` no-escape flag.
pub const GLOB_NOESCAPE: i64 = 4096;
/// `glob()` error flag.
pub const GLOB_ERR: i64 = 4;
/// `glob()` directories-only flag.
pub const GLOB_ONLYDIR: i64 = 1_073_741_824;
/// `pathinfo()` dirname selector.
pub const PATHINFO_DIRNAME: i64 = 1;
/// `pathinfo()` basename selector.
pub const PATHINFO_BASENAME: i64 = 2;
/// `pathinfo()` extension selector.
pub const PATHINFO_EXTENSION: i64 = 4;
/// `pathinfo()` filename selector.
pub const PATHINFO_FILENAME: i64 = 8;
/// INI setting may be changed in user scripts.
pub const INI_USER: i64 = 1;
/// INI setting may be changed in directory-level configuration.
pub const INI_PERDIR: i64 = 2;
/// INI setting may be changed in system-level configuration.
pub const INI_SYSTEM: i64 = 4;
/// INI setting may be changed in every supported context.
pub const INI_ALL: i64 = INI_USER | INI_PERDIR | INI_SYSTEM;
/// Normal INI scanner mode.
pub const INI_SCANNER_NORMAL: i64 = 0;
/// Raw INI scanner mode.
pub const INI_SCANNER_RAW: i64 = 1;
/// Typed INI scanner mode.
pub const INI_SCANNER_TYPED: i64 = 2;
/// `fnmatch()` no-escape flag.
pub const FNM_NOESCAPE: i64 = 1;
/// `fnmatch()` pathname flag.
pub const FNM_PATHNAME: i64 = 2;
/// `fnmatch()` period flag.
pub const FNM_PERIOD: i64 = 4;
/// `fnmatch()` case-fold flag.
pub const FNM_CASEFOLD: i64 = 16;
/// HTML entity translation mode.
pub const HTML_ENTITIES: i64 = 1;
/// HTML escaping compatibility quote mode.
pub const ENT_COMPAT: i64 = 2;
/// Default HTML escaping quote mode.
pub const ENT_QUOTES: i64 = 3;
/// HTML escaping no-quotes mode.
pub const ENT_NOQUOTES: i64 = 0;
/// Ignore invalid code units during HTML escaping.
pub const ENT_IGNORE: i64 = 4;
/// Substitute invalid code units during HTML escaping.
pub const ENT_SUBSTITUTE: i64 = 8;
/// Disallow invalid code points during HTML escaping.
pub const ENT_DISALLOWED: i64 = 128;
/// HTML 4.01 document type flag.
pub const ENT_HTML401: i64 = 0;
/// XML 1 document type flag.
pub const ENT_XML1: i64 = 16;
/// XHTML document type flag.
pub const ENT_XHTML: i64 = 32;
/// HTML5 document type flag.
pub const ENT_HTML5: i64 = 48;
/// Maximum signed char value.
pub const CHAR_MAX: i64 = 127;
/// `htmlspecialchars()` escaping mode.
pub const HTML_SPECIALCHARS: i64 = 0;
/// `http_build_query()` RFC 1738 encoding mode.
pub const PHP_QUERY_RFC1738: i64 = 1;
/// `http_build_query()` RFC 3986 encoding mode.
pub const PHP_QUERY_RFC3986: i64 = 2;

/// `parse_url()` component selector for the URL scheme.
pub const PHP_URL_SCHEME: i64 = 0;
/// `parse_url()` component selector for the URL host.
pub const PHP_URL_HOST: i64 = 1;
/// `parse_url()` component selector for the URL port.
pub const PHP_URL_PORT: i64 = 2;
/// `parse_url()` component selector for the URL user.
pub const PHP_URL_USER: i64 = 3;
/// `parse_url()` component selector for the URL password.
pub const PHP_URL_PASS: i64 = 4;
/// `parse_url()` component selector for the URL path.
pub const PHP_URL_PATH: i64 = 5;
/// `parse_url()` component selector for the URL query.
pub const PHP_URL_QUERY: i64 = 6;
/// `parse_url()` component selector for the URL fragment.
pub const PHP_URL_FRAGMENT: i64 = 7;

/// GD GIF image-type bit.
pub const IMG_GIF: i64 = 1;
/// GD JPEG image-type bit.
pub const IMG_JPG: i64 = 2;
/// GD JPEG image-type alias bit.
pub const IMG_JPEG: i64 = 2;
/// GD PNG image-type bit.
pub const IMG_PNG: i64 = 4;
/// GD WebP image-type bit.
pub const IMG_WEBP: i64 = 32;
/// GD AVIF image-type bit.
pub const IMG_AVIF: i64 = 256;

/// Unknown image type.
pub const IMAGETYPE_UNKNOWN: i64 = 0;
/// GIF image type.
pub const IMAGETYPE_GIF: i64 = 1;
/// JPEG image type.
pub const IMAGETYPE_JPEG: i64 = 2;
/// PNG image type.
pub const IMAGETYPE_PNG: i64 = 3;
/// SWF image type.
pub const IMAGETYPE_SWF: i64 = 4;
/// PSD image type.
pub const IMAGETYPE_PSD: i64 = 5;
/// BMP image type.
pub const IMAGETYPE_BMP: i64 = 6;
/// Little-endian TIFF image type.
pub const IMAGETYPE_TIFF_II: i64 = 7;
/// Big-endian TIFF image type.
pub const IMAGETYPE_TIFF_MM: i64 = 8;
/// JPEG2000 codestream image type.
pub const IMAGETYPE_JPC: i64 = 9;
/// JP2 image type.
pub const IMAGETYPE_JP2: i64 = 10;
/// JPX image type.
pub const IMAGETYPE_JPX: i64 = 11;
/// JB2 image type.
pub const IMAGETYPE_JB2: i64 = 12;
/// SWC image type.
pub const IMAGETYPE_SWC: i64 = 13;
/// IFF image type.
pub const IMAGETYPE_IFF: i64 = 14;
/// WBMP image type.
pub const IMAGETYPE_WBMP: i64 = 15;
/// XBM image type.
pub const IMAGETYPE_XBM: i64 = 16;
/// ICO image type.
pub const IMAGETYPE_ICO: i64 = 17;
/// WebP image type.
pub const IMAGETYPE_WEBP: i64 = 18;
/// AVIF image type.
pub const IMAGETYPE_AVIF: i64 = 19;
/// JPEG2000 userland alias for JPC.
pub const IMAGETYPE_JPEG2000: i64 = IMAGETYPE_JPC;
/// HEIF image type.
pub const IMAGETYPE_HEIF: i64 = 20;
/// SVG image type.
pub const IMAGETYPE_SVG: i64 = 21;
/// First dynamic image type id.
pub const IMAGETYPE_COUNT: i64 = 22;

/// PHP `PASSWORD_DEFAULT` algorithm marker.
pub const PASSWORD_DEFAULT: &str = "2y";
/// PHP `PASSWORD_BCRYPT` algorithm marker.
pub const PASSWORD_BCRYPT: &str = "2y";
/// PHP default bcrypt password hashing cost.
pub const PASSWORD_BCRYPT_DEFAULT_COST: i64 = 12;

/// `sort()`/`array_multisort()` ascending order flag.
pub const SORT_ASC: i64 = 4;
/// `sort()`/`array_multisort()` descending order flag.
pub const SORT_DESC: i64 = 3;
/// Regular PHP comparison sort flag.
pub const SORT_REGULAR: i64 = 0;
/// Numeric comparison sort flag.
pub const SORT_NUMERIC: i64 = 1;
/// String comparison sort flag.
pub const SORT_STRING: i64 = 2;
/// Locale-aware string comparison sort flag.
pub const SORT_LOCALE_STRING: i64 = 5;
/// Natural string comparison sort flag.
pub const SORT_NATURAL: i64 = 6;
/// Case-insensitive string/natural sort modifier.
pub const SORT_FLAG_CASE: i64 = 8;

/// Locale category for all locale settings.
pub const LC_ALL: i64 = 0;
/// Locale category for character classification and conversion.
pub const LC_CTYPE: i64 = 2;
/// Locale category for numeric formatting.
pub const LC_NUMERIC: i64 = 4;
/// Locale category for date/time formatting.
pub const LC_TIME: i64 = 5;
/// Locale category for string collation.
pub const LC_COLLATE: i64 = 1;
/// Locale category for monetary formatting.
pub const LC_MONETARY: i64 = 3;
/// Locale category for localized messages.
pub const LC_MESSAGES: i64 = 6;

/// Lowercase key conversion flag.
pub const CASE_LOWER: i64 = 0;
/// Uppercase key conversion flag.
pub const CASE_UPPER: i64 = 1;
/// Non-recursive count mode.
pub const COUNT_NORMAL: i64 = 0;
/// Recursive count mode.
pub const COUNT_RECURSIVE: i64 = 1;
/// `array_filter()` callback receives value and key.
pub const ARRAY_FILTER_USE_BOTH: i64 = 1;
/// `array_filter()` callback receives key.
pub const ARRAY_FILTER_USE_KEY: i64 = 2;

/// DateTimeInterface::ATOM date format.
pub const DATE_ATOM: &str = "Y-m-d\\TH:i:sP";
/// Cookie date format.
pub const DATE_COOKIE: &str = "l, d-M-Y H:i:s T";
/// ISO-8601 date format.
pub const DATE_ISO8601: &str = "Y-m-d\\TH:i:sO";
/// Expanded ISO-8601 date format.
pub const DATE_ISO8601_EXPANDED: &str = "X-m-d\\TH:i:sP";
/// RFC 1036 date format.
pub const DATE_RFC1036: &str = "D, d M y H:i:s O";
/// RFC 1123 date format.
pub const DATE_RFC1123: &str = "D, d M Y H:i:s O";
/// RFC 2822 date format.
pub const DATE_RFC2822: &str = "D, d M Y H:i:s O";
/// RFC 3339 date format.
pub const DATE_RFC3339: &str = "Y-m-d\\TH:i:sP";
/// RFC 3339 extended date format.
pub const DATE_RFC3339_EXTENDED: &str = "Y-m-d\\TH:i:s.vP";
/// RFC 7231 date format.
pub const DATE_RFC7231: &str = "D, d M Y H:i:s \\G\\M\\T";
/// RFC 822 date format.
pub const DATE_RFC822: &str = "D, d M y H:i:s O";
/// RFC 850 date format.
pub const DATE_RFC850: &str = "l, d-M-y H:i:s T";
/// RSS date format.
pub const DATE_RSS: &str = DATE_RFC1123;
/// W3C date format.
pub const DATE_W3C: &str = DATE_RFC3339;

/// `str_pad()` left padding selector.
pub const STR_PAD_LEFT: i64 = 0;
/// `str_pad()` right padding selector.
pub const STR_PAD_RIGHT: i64 = 1;
/// `str_pad()` both-sides padding selector.
pub const STR_PAD_BOTH: i64 = 2;

/// Successful file upload.
pub const UPLOAD_ERR_OK: i64 = 0;
/// Uploaded file exceeded the configured upload max filesize.
pub const UPLOAD_ERR_INI_SIZE: i64 = 1;
/// Uploaded file exceeded the form-specified max filesize.
pub const UPLOAD_ERR_FORM_SIZE: i64 = 2;
/// Uploaded file was only partially received.
pub const UPLOAD_ERR_PARTIAL: i64 = 3;
/// No file was uploaded.
pub const UPLOAD_ERR_NO_FILE: i64 = 4;
/// Missing temporary upload directory.
pub const UPLOAD_ERR_NO_TMP_DIR: i64 = 6;
/// Uploaded file could not be written to disk.
pub const UPLOAD_ERR_CANT_WRITE: i64 = 7;
/// Upload stopped by an extension.
pub const UPLOAD_ERR_EXTENSION: i64 = 8;

/// Converts registry constant metadata into a runtime value.
#[must_use]
pub fn constant_to_value(value: ConstantValue) -> Value {
    match value {
        ConstantValue::Null => Value::Null,
        ConstantValue::Bool(value) => Value::Bool(value),
        ConstantValue::Int(value) => Value::Int(value),
        ConstantValue::Float(value) => Value::Float(value),
        ConstantValue::String(value) => Value::String(PhpString::from(value)),
        ConstantValue::Array(values) => {
            Value::packed_array(values.iter().copied().map(constant_to_value).collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ExtensionRegistry;

    #[test]
    fn version_constants_match_foundation_target() {
        assert_eq!(PHP_VERSION, "8.5.7");
        assert_eq!(PHP_VERSION_ID, 80507);
        assert_eq!(PHP_MAJOR_VERSION, 8);
        assert_eq!(PHP_MINOR_VERSION, 5);
        assert_eq!(PHP_RELEASE_VERSION, 7);
        assert_eq!(PHP_INT_SIZE, std::mem::size_of::<isize>() as i64);
        assert_eq!(PHP_INT_MAX, isize::MAX as i64);
        assert_eq!(PHP_INT_MIN, isize::MIN as i64);
        assert!(INF.to_f64().is_infinite());
        assert!(NAN.to_f64().is_nan());
    }

    #[test]
    fn core_constants_are_registered_with_values() {
        let registry = ExtensionRegistry::standard_library();
        let version_id = registry
            .enabled_constant("PHP_VERSION_ID")
            .expect("PHP_VERSION_ID");
        assert_eq!(version_id.value(), Some(ConstantValue::Int(80507)));

        for (name, expected_extension) in [
            ("TRUE", "core"),
            ("FALSE", "core"),
            ("NULL", "core"),
            ("PHP_EXTENSION_DIR", "core"),
            ("PEAR_EXTENSION_DIR", "core"),
            ("PEAR_INSTALL_DIR", "core"),
            ("PHP_BINDIR", "core"),
            ("PHP_BUILD_DATE", "core"),
            ("PHP_CLI_PROCESS_TITLE", "core"),
            ("PHP_CONFIG_FILE_PATH", "core"),
            ("PHP_CONFIG_FILE_SCAN_DIR", "core"),
            ("PHP_DATADIR", "core"),
            ("PHP_DEBUG", "core"),
            ("PHP_EXTRA_VERSION", "core"),
            ("PHP_FD_SETSIZE", "core"),
            ("PHP_LIBDIR", "core"),
            ("PHP_LOCALSTATEDIR", "core"),
            ("PHP_MANDIR", "core"),
            ("PHP_PREFIX", "core"),
            ("PHP_SBINDIR", "core"),
            ("PHP_SHLIB_SUFFIX", "core"),
            ("PHP_SYSCONFDIR", "core"),
            ("PHP_ZTS", "core"),
            ("ZEND_DEBUG_BUILD", "core"),
            ("ZEND_THREAD_SAFE", "core"),
            ("ZEND_VM_KIND", "core"),
            ("PHP_OUTPUT_HANDLER_CLEAN", "core"),
            ("PHP_OUTPUT_HANDLER_CLEANABLE", "core"),
            ("PHP_OUTPUT_HANDLER_CONT", "core"),
            ("PHP_OUTPUT_HANDLER_DISABLED", "core"),
            ("PHP_OUTPUT_HANDLER_END", "core"),
            ("PHP_OUTPUT_HANDLER_FINAL", "core"),
            ("PHP_OUTPUT_HANDLER_FLUSH", "core"),
            ("PHP_OUTPUT_HANDLER_FLUSHABLE", "core"),
            ("PHP_OUTPUT_HANDLER_PROCESSED", "core"),
            ("PHP_OUTPUT_HANDLER_REMOVABLE", "core"),
            ("PHP_OUTPUT_HANDLER_START", "core"),
            ("PHP_OUTPUT_HANDLER_STARTED", "core"),
            ("PHP_OUTPUT_HANDLER_STDFLAGS", "core"),
            ("PHP_OUTPUT_HANDLER_WRITE", "core"),
            ("PHP_FLOAT_DIG", "core"),
            ("PHP_FLOAT_EPSILON", "core"),
            ("PHP_FLOAT_MAX", "core"),
            ("PHP_FLOAT_MIN", "core"),
            ("STDIN", "core"),
            ("STDOUT", "core"),
            ("STDERR", "core"),
            ("UPLOAD_ERR_OK", "core"),
            ("UPLOAD_ERR_EXTENSION", "core"),
            ("DIRECTORY_SEPARATOR", "standard"),
            ("PATH_SEPARATOR", "standard"),
            ("INF", "standard"),
            ("NAN", "standard"),
        ] {
            assert_eq!(
                registry
                    .enabled_constant(name)
                    .map(crate::ConstantDescriptor::extension),
                Some(expected_extension),
                "{name} should be registered under the php-src owner extension"
            );
        }

        assert_eq!(
            registry
                .enabled_constant("TRUE")
                .and_then(crate::ConstantDescriptor::value),
            Some(ConstantValue::Bool(true))
        );
        assert_eq!(
            registry
                .enabled_constant("FALSE")
                .and_then(crate::ConstantDescriptor::value),
            Some(ConstantValue::Bool(false))
        );
        assert_eq!(
            registry
                .enabled_constant("NULL")
                .and_then(crate::ConstantDescriptor::value),
            Some(ConstantValue::Null)
        );

        let separator = registry
            .enabled_constant("DIRECTORY_SEPARATOR")
            .expect("DIRECTORY_SEPARATOR");
        assert_eq!(
            constant_to_value(separator.value().expect("separator value")),
            Value::String(PhpString::from(DIRECTORY_SEPARATOR))
        );

        assert_eq!(
            registry
                .enabled_constant("PHP_INT_MAX")
                .and_then(crate::ConstantDescriptor::value),
            Some(ConstantValue::Int(PHP_INT_MAX))
        );
        assert_eq!(
            registry
                .enabled_constant("PHP_INT_SIZE")
                .and_then(crate::ConstantDescriptor::value),
            Some(ConstantValue::Int(PHP_INT_SIZE))
        );
        assert_eq!(
            registry
                .enabled_constant("PEAR_EXTENSION_DIR")
                .and_then(crate::ConstantDescriptor::value),
            Some(ConstantValue::String(PEAR_EXTENSION_DIR))
        );
        assert_eq!(
            registry
                .enabled_constant("PHP_DEBUG")
                .and_then(crate::ConstantDescriptor::value),
            Some(ConstantValue::Bool(PHP_DEBUG))
        );
        assert_eq!(
            registry
                .enabled_constant("PHP_OUTPUT_HANDLER_STDFLAGS")
                .and_then(crate::ConstantDescriptor::value),
            Some(ConstantValue::Int(PHP_OUTPUT_HANDLER_STDFLAGS))
        );
        assert_eq!(
            registry
                .enabled_constant("ZEND_VM_KIND")
                .and_then(crate::ConstantDescriptor::value),
            Some(ConstantValue::String(ZEND_VM_KIND))
        );
        assert!(matches!(
            registry
                .enabled_constant("INF")
                .and_then(crate::ConstantDescriptor::value),
            Some(ConstantValue::Float(value)) if value.to_f64().is_infinite()
        ));
        assert!(matches!(
            registry
                .enabled_constant("NAN")
                .and_then(crate::ConstantDescriptor::value),
            Some(ConstantValue::Float(value)) if value.to_f64().is_nan()
        ));
        assert_eq!(
            registry
                .enabled_constant("INI_ALL")
                .and_then(crate::ConstantDescriptor::value),
            Some(ConstantValue::Int(7))
        );
        assert_eq!(
            registry
                .enabled_constant("INI_USER")
                .and_then(crate::ConstantDescriptor::value),
            Some(ConstantValue::Int(1))
        );
        assert_eq!(
            registry
                .enabled_constant("INI_PERDIR")
                .and_then(crate::ConstantDescriptor::value),
            Some(ConstantValue::Int(2))
        );
        assert_eq!(
            registry
                .enabled_constant("INI_SYSTEM")
                .and_then(crate::ConstantDescriptor::value),
            Some(ConstantValue::Int(4))
        );
    }

    #[test]
    fn constant_value_metadata_supports_php_scalar_and_array_shapes() {
        static ARRAY_CONSTANT: &[ConstantValue] = &[
            ConstantValue::Null,
            ConstantValue::Bool(true),
            ConstantValue::Int(42),
            ConstantValue::Float(FloatValue::from_f64(1.5)),
            ConstantValue::String("x"),
        ];

        let null = constant_to_value(ConstantValue::Null);
        assert_eq!(null, Value::Null);

        let array = constant_to_value(ConstantValue::Array(ARRAY_CONSTANT));
        let elements = array
            .packed_elements()
            .expect("array constant should be packed");
        assert_eq!(elements[0], &Value::Null);
        assert_eq!(elements[1], &Value::Bool(true));
        assert_eq!(elements[2], &Value::Int(42));
        assert_eq!(elements[3], &Value::Float(FloatValue::from_f64(1.5)));
        assert_eq!(elements[4], &Value::String(PhpString::from("x")));
    }

    #[test]
    fn standard_array_constants_match_php_src_values() {
        assert_eq!(SORT_REGULAR, 0);
        assert_eq!(SORT_NUMERIC, 1);
        assert_eq!(SORT_STRING, 2);
        assert_eq!(SORT_DESC, 3);
        assert_eq!(SORT_ASC, 4);
        assert_eq!(SORT_LOCALE_STRING, 5);
        assert_eq!(SORT_NATURAL, 6);
        assert_eq!(SORT_FLAG_CASE, 8);
        assert_eq!(LC_ALL, 0);
        assert_eq!(LC_COLLATE, 1);
        assert_eq!(LC_CTYPE, 2);
        assert_eq!(LC_MONETARY, 3);
        assert_eq!(LC_NUMERIC, 4);
        assert_eq!(LC_TIME, 5);
        assert_eq!(LC_MESSAGES, 6);
        assert_eq!(CASE_LOWER, 0);
        assert_eq!(CASE_UPPER, 1);
        assert_eq!(COUNT_NORMAL, 0);
        assert_eq!(COUNT_RECURSIVE, 1);
        assert_eq!(ARRAY_FILTER_USE_BOTH, 1);
        assert_eq!(ARRAY_FILTER_USE_KEY, 2);
        assert_eq!(STR_PAD_LEFT, 0);
        assert_eq!(STR_PAD_RIGHT, 1);
        assert_eq!(STR_PAD_BOTH, 2);
        assert_eq!(PHP_ROUND_HALF_UP, 1);
        assert_eq!(PHP_ROUND_HALF_DOWN, 2);
        assert_eq!(PHP_ROUND_HALF_EVEN, 3);
        assert_eq!(PHP_ROUND_HALF_ODD, 4);
        assert_eq!(UPLOAD_ERR_OK, 0);
        assert_eq!(UPLOAD_ERR_INI_SIZE, 1);
        assert_eq!(UPLOAD_ERR_FORM_SIZE, 2);
        assert_eq!(UPLOAD_ERR_PARTIAL, 3);
        assert_eq!(UPLOAD_ERR_NO_FILE, 4);
        assert_eq!(UPLOAD_ERR_NO_TMP_DIR, 6);
        assert_eq!(UPLOAD_ERR_CANT_WRITE, 7);
        assert_eq!(UPLOAD_ERR_EXTENSION, 8);
        assert_eq!(IMAGETYPE_UNKNOWN, 0);
        assert_eq!(IMAGETYPE_GIF, 1);
        assert_eq!(IMAGETYPE_JPEG, 2);
        assert_eq!(IMAGETYPE_PNG, 3);
        assert_eq!(IMAGETYPE_TIFF_II, 7);
        assert_eq!(IMAGETYPE_TIFF_MM, 8);
        assert_eq!(IMAGETYPE_JPC, 9);
        assert_eq!(IMAGETYPE_JPEG2000, IMAGETYPE_JPC);
        assert_eq!(IMAGETYPE_WEBP, 18);
        assert_eq!(IMAGETYPE_AVIF, 19);
        assert_eq!(IMAGETYPE_HEIF, 20);
        assert_eq!(IMAGETYPE_SVG, 21);
        assert_eq!(IMAGETYPE_COUNT, 22);
        assert_eq!(M_PI.to_f64(), std::f64::consts::PI);
        assert_eq!(M_SQRTPI.to_f64(), 1.772453850905516);
        assert_eq!(M_EULER.to_f64(), 0.5772156649015329);
        assert_eq!(M_SQRT3.to_f64(), 1.7320508075688772);
        assert_eq!(CAL_GREGORIAN, 0);
        assert_eq!(CAL_JULIAN, 1);
        assert_eq!(CAL_JEWISH, 2);
        assert_eq!(CAL_FRENCH, 3);
        assert_eq!(CAL_NUM_CALS, 4);
        assert_eq!(CAL_DOW_DAYNO, 0);
        assert_eq!(CAL_DOW_LONG, 1);
        assert_eq!(CAL_DOW_SHORT, 2);
        assert_eq!(CAL_MONTH_GREGORIAN_SHORT, 0);
        assert_eq!(CAL_MONTH_GREGORIAN_LONG, 1);
        assert_eq!(CAL_MONTH_JULIAN_SHORT, 2);
        assert_eq!(CAL_MONTH_JULIAN_LONG, 3);
        assert_eq!(CAL_MONTH_JEWISH, 4);
        assert_eq!(CAL_MONTH_FRENCH, 5);
        assert_eq!(CAL_EASTER_DEFAULT, 0);
        assert_eq!(CAL_EASTER_ROMAN, 1);
        assert_eq!(CAL_EASTER_ALWAYS_GREGORIAN, 2);
        assert_eq!(CAL_EASTER_ALWAYS_JULIAN, 3);
        assert_eq!(CAL_JEWISH_ADD_ALAFIM_GERESH, 2);
        assert_eq!(CAL_JEWISH_ADD_ALAFIM, 4);
        assert_eq!(CAL_JEWISH_ADD_GERESHAYIM, 8);
    }

    #[test]
    fn json_constants_are_enabled_with_json_extension() {
        let mut registry = ExtensionRegistry::standard_library().clone();
        assert_eq!(
            registry
                .enabled_constant("JSON_ERROR_NON_BACKED_ENUM")
                .and_then(crate::ConstantDescriptor::value),
            Some(ConstantValue::Int(11))
        );

        registry.disable_extension("json").expect("disable json");
        assert!(
            registry
                .enabled_constant("JSON_ERROR_NON_BACKED_ENUM")
                .is_none()
        );
        registry.enable_extension("json").expect("re-enable json");
        assert_eq!(
            registry
                .enabled_constant("JSON_ERROR_NON_BACKED_ENUM")
                .and_then(crate::ConstantDescriptor::value),
            Some(ConstantValue::Int(11))
        );
    }
}

--TEST--
mbstring: expanded encoding registry aliases and conversions
--DESCRIPTION--
Focused MB-1 coverage for UTF-8, 8BIT/Binary, ASCII, ISO-8859-1,
Windows-1252, SJIS, EUC-JP, and ISO-2022-JP registry, conversion,
detection, checking, and invalid encoding behavior.
--SKIPIF--
<?php
if (!extension_loaded("mbstring")) {
    die("skip mbstring extension not loaded\n");
}
?>
--FILE--
<?php
foreach (["UTF-8", "8bit", "ASCII", "ISO-8859-1", "Windows-1252", "SJIS", "EUC-JP", "ISO-2022-JP"] as $encoding) {
    var_dump(in_array($encoding, mb_list_encodings(), true));
}

var_dump(in_array("binary", mb_encoding_aliases("8bit"), true));
var_dump(in_array("EUC_JP", mb_encoding_aliases("EUC-JP"), true));
var_dump(count(mb_encoding_aliases("ISO-2022-JP")));

$jp = "日本";
$euc = mb_convert_encoding($jp, "EUC-JP", "UTF-8");
$jis = mb_convert_encoding($jp, "ISO-2022-JP", "UTF-8");
var_dump(bin2hex($euc));
var_dump(bin2hex($jis));
var_dump(mb_strlen($euc, "EUC-JP"));
var_dump(mb_strlen($jis, "ISO-2022-JP"));
var_dump(mb_detect_encoding($euc, ["ASCII", "EUC-JP"], true));
var_dump(mb_detect_encoding($jis, ["ASCII", "ISO-2022-JP"], true));
var_dump(bin2hex(mb_substr($euc, 1, 1, "EUC-JP")));
var_dump(bin2hex(mb_substr($jis, 1, 1, "ISO-2022-JP")));
var_dump(mb_check_encoding("\xff", "8bit"));
var_dump(mb_detect_encoding("\xff", ["ASCII", "8bit"], true));
var_dump(bin2hex(mb_convert_encoding("\xff", "UTF-8", "Binary")));
var_dump(bin2hex(mb_convert_encoding("ÿ", "8bit", "UTF-8")));

foreach ([
    fn() => mb_detect_encoding("abc", ["X-UNKNOWN"], true),
    fn() => mb_check_encoding("abc", "X-UNKNOWN"),
    fn() => mb_internal_encoding("X-UNKNOWN"),
    fn() => mb_convert_encoding("abc", "X-UNKNOWN", "UTF-8"),
] as $call) {
    try {
        $call();
    } catch (Throwable $e) {
        echo get_class($e), ": ", $e->getMessage(), "\n";
    }
}
?>
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
int(0)
string(8) "c6fccbdc"
string(20) "1b2442467c4b5c1b2842"
int(2)
int(2)
string(6) "EUC-JP"
string(11) "ISO-2022-JP"
string(4) "cbdc"
string(16) "1b24424b5c1b2842"
bool(true)
bool(false)
string(4) "c3bf"
string(2) "ff"
ValueError: mb_detect_encoding(): Argument #2 ($encodings) contains invalid encoding "X-UNKNOWN"
ValueError: mb_check_encoding(): Argument #2 ($encoding) must be a valid encoding, "X-UNKNOWN" given
ValueError: mb_internal_encoding(): Argument #1 ($encoding) must be a valid encoding, "X-UNKNOWN" given
ValueError: mb_convert_encoding(): Argument #2 ($to_encoding) must be a valid encoding, "X-UNKNOWN" given

--TEST--
mbstring: common string helpers, width, case, ord/chr, parse_str
--DESCRIPTION--
Focused MB-2 coverage for common mbstring helpers that do not require
mbregex/Oniguruma.
--SKIPIF--
<?php
if (!extension_loaded("mbstring")) {
    die("skip mbstring extension not loaded\n");
}
?>
--FILE--
<?php
foreach (["mb_strcut", "mb_strwidth", "mb_strimwidth", "mb_convert_case", "mb_ucfirst", "mb_lcfirst", "mb_ord", "mb_chr", "mb_parse_str"] as $function) {
    var_dump(function_exists($function));
}
foreach (["MB_CASE_UPPER", "MB_CASE_LOWER", "MB_CASE_TITLE", "MB_CASE_FOLD", "MB_CASE_UPPER_SIMPLE", "MB_CASE_LOWER_SIMPLE", "MB_CASE_TITLE_SIMPLE", "MB_CASE_FOLD_SIMPLE"] as $constant) {
    echo $constant, "=", constant($constant), "\n";
}
$s = "Aé日😀";
var_dump(mb_strcut($s, 1, 4, "UTF-8"));
var_dump(bin2hex(mb_strcut($s, 1, 4, "8bit")));
var_dump(mb_strwidth($s, "UTF-8"));
var_dump(mb_strimwidth($s, 0, 5, "..", "UTF-8"));
var_dump(mb_convert_case("aBc élan", MB_CASE_UPPER, "UTF-8"));
var_dump(mb_convert_case("aBc élan", MB_CASE_LOWER, "UTF-8"));
var_dump(mb_convert_case("hello world", MB_CASE_TITLE, "UTF-8"));
var_dump(mb_ucfirst("élan", "UTF-8"));
var_dump(mb_lcfirst("Élan", "UTF-8"));
var_dump(mb_ord("😀", "UTF-8"));
var_dump(bin2hex(mb_chr(0x1f600, "UTF-8")));
$out = [];
var_dump(mb_parse_str("a=1&b%5B%5D=2", $out));
var_dump($out);
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
MB_CASE_UPPER=0
MB_CASE_LOWER=1
MB_CASE_TITLE=2
MB_CASE_FOLD=3
MB_CASE_UPPER_SIMPLE=4
MB_CASE_LOWER_SIMPLE=5
MB_CASE_TITLE_SIMPLE=6
MB_CASE_FOLD_SIMPLE=7
string(2) "é"
string(8) "c3a9e697"
int(6)
string(5) "Aé.."
string(9) "ABC ÉLAN"
string(9) "abc élan"
string(11) "Hello World"
string(5) "Élan"
string(5) "élan"
int(128512)
string(8) "f09f9880"
bool(true)
array(2) {
  ["a"]=>
  string(1) "1"
  ["b"]=>
  array(1) {
    [0]=>
    string(1) "2"
  }
}

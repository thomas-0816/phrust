--TEST--
mbstring: bounded UTF-8 encoding state and detection
--DESCRIPTION--
Focused mbstring UTF-8 coverage for internal encoding,
mb_detect_encoding, mb_check_encoding, and narrow UTF-8 mb_convert_encoding.
Reference output captured from PHP 8.5.7 php-src with --enable-mbstring
--disable-mbregex.
--FILE--
<?php
var_dump(mb_internal_encoding());
var_dump(mb_internal_encoding("ASCII"));
var_dump(mb_internal_encoding());
var_dump(mb_internal_encoding("UTF-8"));
var_dump(mb_detect_encoding("Aé日", ["ASCII", "UTF-8"], true));
var_dump(mb_detect_encoding("abc", ["ASCII", "UTF-8"], true));
var_dump(mb_check_encoding("Aé日", "UTF-8"));
var_dump(mb_check_encoding("abc", "ASCII"));
var_dump(mb_convert_encoding("Aé日", "UTF-8", "UTF-8"));
?>
--EXPECT--
string(5) "UTF-8"
bool(true)
string(5) "ASCII"
bool(true)
string(5) "UTF-8"
string(5) "ASCII"
bool(true)
bool(true)
string(6) "Aé日"

--TEST--
iconv_strrpos selected encodings
--SKIPIF--
<?php if (!extension_loaded("iconv")) die("skip iconv extension not loaded"); ?>
--FILE--
<?php
var_dump(iconv_strrpos("abecdbcdabcdef", "bcd"));

$ascii = str_repeat("abcab", 60) . "abcdb" . str_repeat("adabc", 60);
var_dump(iconv_strlen($ascii));
var_dump(iconv_strrpos($ascii, "abcd"));

$euc_jp = "\xC6\xFC\xCB\xDC\xC6\xFC\xCB\xDC";
var_dump(iconv_strlen($euc_jp, "EUC-JP"));
var_dump(iconv_strrpos($euc_jp, "\xCB\xDC", "EUC-JP"));

var_dump(iconv_strrpos("string", ""));
var_dump(iconv_strrpos("", "string"));
?>
--EXPECT--
int(9)
int(605)
int(300)
int(4)
int(3)
bool(false)
bool(false)

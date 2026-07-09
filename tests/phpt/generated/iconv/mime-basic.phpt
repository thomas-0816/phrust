--TEST--
iconv MIME helpers selected basics
--SKIPIF--
<?php if (!extension_loaded("iconv")) die("skip iconv extension not loaded"); ?>
--FILE--
<?php
var_dump(ICONV_MIME_DECODE_STRICT, ICONV_MIME_DECODE_CONTINUE_ON_ERROR);
var_dump(iconv_mime_encode("Subject", "hello"));
var_dump(iconv_mime_encode("Subject", "Pr\xC3\xBCfung", [
    "input-charset" => "UTF-8",
    "output-charset" => "UTF-8",
    "scheme" => "B",
]));
var_dump(iconv_mime_encode("Subject", "Pr\xC3\xBCfung", [
    "input-charset" => "UTF-8",
    "output-charset" => "UTF-8",
    "scheme" => "Q",
]));
var_dump(bin2hex(iconv_mime_decode("=?utf-8?B?UHLDvGZ1bmc=?=", 0, "UTF-8")));
var_dump(bin2hex(iconv_mime_decode("Subject: =?utf-8?Q?Pr=C3=BCfung?=", 0, "UTF-8")));
var_dump(iconv_mime_decode("=?UTF-8?Q?hello_world?=", 0, "UTF-8"));
?>
--EXPECT--
int(1)
int(2)
string(29) "Subject: =?UTF-8?B?aGVsbG8=?="
string(33) "Subject: =?UTF-8?B?UHLDvGZ1bmc=?="
string(33) "Subject: =?UTF-8?Q?Pr=C3=BCfung?="
string(16) "5072c3bc66756e67"
string(34) "5375626a6563743a205072c3bc66756e67"
string(11) "hello world"

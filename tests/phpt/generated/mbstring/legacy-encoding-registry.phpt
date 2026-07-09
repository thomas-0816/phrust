--TEST--
mbstring: legacy encoding registry conversion and string helpers
--DESCRIPTION--
Focused mbstring coverage for ISO-8859-1, Windows-1252, and Shift_JIS
conversion, detection, checking, length, substring, position, registry alias,
and substitute-character helpers.
Expected output was checked against a mbstring-enabled PHP 8.5 CLI; the pinned
project reference build skips this fixture when mbstring is unavailable.
--SKIPIF--
<?php
if (!extension_loaded("mbstring")) {
    die("skip mbstring extension not loaded\n");
}
?>
--FILE--
<?php
$latin1 = "R\xE9sum\xE9";
var_dump(mb_convert_encoding($latin1, "UTF-8", "ISO-8859-1"));
var_dump(bin2hex(mb_convert_encoding("Résumé", "ISO-8859-1", "UTF-8")));

$sjis = "\x93\xFA\x96\x7B";
var_dump(mb_strlen($sjis, "SJIS"));
var_dump(mb_convert_encoding($sjis, "UTF-8", "SJIS"));
var_dump(mb_detect_encoding($sjis, ["ASCII", "SJIS"], true));
var_dump(mb_check_encoding($sjis, "SJIS"));
var_dump(bin2hex(mb_substr($sjis, 1, 1, "SJIS")));
var_dump(mb_strpos($sjis, "\x96\x7B", 0, "SJIS"));

$win1252 = "\x80";
var_dump(mb_convert_encoding($win1252, "UTF-8", "Windows-1252"));

var_dump(in_array("UTF-8", mb_list_encodings(), true));
var_dump(in_array("SJIS", mb_list_encodings(), true));
var_dump(in_array("ISO-8859-1", mb_list_encodings(), true));
var_dump(in_array("Windows-1252", mb_list_encodings(), true));
var_dump(in_array("SHIFT-JIS", mb_encoding_aliases("SJIS"), true));
var_dump(in_array("latin1", mb_encoding_aliases("ISO-8859-1"), true));
var_dump(mb_substitute_character());
var_dump(mb_substitute_character("none"));
var_dump(mb_substitute_character());
var_dump(mb_substitute_character(0x3f));
var_dump(mb_substitute_character());
try {
    mb_substitute_character("bad");
} catch (Throwable $e) {
    echo get_class($e), ": ", $e->getMessage(), "\n";
}
?>
--EXPECT--
string(8) "Résumé"
string(12) "52e973756de9"
int(2)
string(6) "日本"
string(4) "SJIS"
bool(true)
string(4) "967b"
int(1)
string(3) "€"
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
int(63)
bool(true)
string(4) "none"
bool(true)
int(63)
ValueError: mb_substitute_character(): Argument #1 ($substitute_character) must be "none", "long", "entity" or a valid codepoint

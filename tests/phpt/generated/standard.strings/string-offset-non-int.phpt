--TEST--
Generated standard.strings: non-integer string offsets warn or throw
--DESCRIPTION--
module: standard.strings
generated timestamp: 20260626T000000Z
generator version: phpt-standard-strings-v1
reason: leading-integer string offsets warn "Illegal string offset" and read the leading index, isset of a non-numeric offset is false, and reading a non-numeric string offset throws TypeError (tests/strings/offsets_chaining_5.phpt, offsets_general.phpt)
--FILE--
<?php
$s = "foobar";
var_dump($s["0foo"]);
var_dump(isset($s["foo"]));
try {
    var_dump($s["foo"]);
} catch (\TypeError $e) {
    echo $e->getMessage(), "\n";
}
?>
--EXPECTF--
Warning: Illegal string offset "0foo" in %s on line %d
string(1) "f"
bool(false)
Cannot access offset of type string on string

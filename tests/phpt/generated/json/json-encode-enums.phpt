--TEST--
json: encode enum cases
--DESCRIPTION--
Generated focused coverage for json_encode() backed and non-backed enum case parity.
--FILE--
<?php
enum UnitCase {
    case A;
}

enum BackedCase: string {
    case A = "x";
}

var_dump(JSON_ERROR_NON_BACKED_ENUM);
var_dump(json_encode(UnitCase::A));
var_dump(json_last_error());
var_dump(json_last_error_msg());
var_dump(json_encode(UnitCase::A, JSON_PARTIAL_OUTPUT_ON_ERROR));
var_dump(json_last_error());
var_dump(json_last_error_msg());
var_dump(json_encode(BackedCase::A));
var_dump(json_last_error());
var_dump(json_last_error_msg());
?>
--EXPECT--
int(11)
bool(false)
int(11)
string(46) "Non-backed enums have no default serialization"
string(1) "0"
int(11)
string(46) "Non-backed enums have no default serialization"
string(3) ""x""
int(0)
string(8) "No error"

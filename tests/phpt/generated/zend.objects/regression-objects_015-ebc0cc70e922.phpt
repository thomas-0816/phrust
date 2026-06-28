--TEST--
PHPT generated regression: comparing objects with strings/NULL
--DESCRIPTION--
original php-src path: Zend/tests/objects/objects_015.phpt
original source hash: ebc0cc70e922b7bffac381fd0536e9dd354fc1321c0f4365c5acf05e2c467461
generated timestamp: 20260627T201250Z
generator version: phpt-generate-v1
reason: known target failure minimized against reference output
--FILE--
<?php
$o=new stdClass;
var_dump($o == "");
var_dump($o != "");
var_dump($o <  "");
var_dump("" <  $o);
var_dump("" >  $o);
var_dump($o != null);
var_dump(is_null($o));
--EXPECT--
bool(false)
bool(true)
bool(false)
bool(true)
bool(false)
bool(true)
bool(false)

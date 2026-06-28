--TEST--
standard.serialization: serialized reference records remain an explicit gap
--FILE--
<?php
error_reporting(0);
var_dump(@unserialize('R:1;'));
var_dump(@unserialize('r:1;'));
?>
--EXPECT--
bool(false)
bool(false)

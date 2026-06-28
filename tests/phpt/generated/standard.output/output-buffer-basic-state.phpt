--TEST--
standard.output: basic output buffer state
--FILE--
<?php
var_dump(ob_get_contents());
var_dump(ob_get_length());
var_dump(ob_get_level());
ob_start();
echo "abc";
$contents = ob_get_contents();
$length = ob_get_length();
$level = ob_get_level();
$clean = ob_get_clean();
var_dump($contents, $length, $level, $clean, ob_get_level());
?>
--EXPECT--
bool(false)
bool(false)
int(0)
string(3) "abc"
int(3)
int(1)
string(3) "abc"
int(0)

--TEST--
standard.output: nested output buffers can be cleaned independently
--FILE--
<?php
echo "root|";
ob_start();
echo "outer";
ob_start();
echo "inner";
$level = ob_get_level();
$length = ob_get_length();
$inner = ob_get_clean();
echo ":after-inner:";
$outer = ob_get_clean();
var_dump($level, $length, $inner, $outer);
echo "|done";
?>
--EXPECT--
root|int(2)
int(5)
string(5) "inner"
string(18) "outer:after-inner:"
|done

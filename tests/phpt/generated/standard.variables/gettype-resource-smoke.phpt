--TEST--
standard.variables: gettype resource smoke
--DESCRIPTION--
Generated focused coverage for gettype() resource names without settype().
--FILE--
<?php
$open = fopen(__FILE__, "r");
$closed = fopen(__FILE__, "r");
fclose($closed);

var_dump(gettype($open));
var_dump(gettype($closed));

fclose($open);
?>
--EXPECT--
string(8) "resource"
string(17) "resource (closed)"

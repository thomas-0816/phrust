--TEST--
Generated zend.functions: user defaults and by-value args are stable
--DESCRIPTION--
module: zend.functions
generated timestamp: 20260627T000000Z
generator version: phpt-zend-functions-v1
reason: user-function defaults and by-value argument binding
--FILE--
<?php
function defaults($a = "A", $b = "B", $c = "C")
{
    echo $a, $b, $c, "\n";
}
function by_value($value)
{
    $value = "changed";
}
$original = "kept";
defaults();
defaults("X", "Y");
by_value($original);
echo $original, "\n";
?>
--EXPECT--
ABC
XYC
kept

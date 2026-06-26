--TEST--
Generated standard.arrays: substr_replace replacement array skips unset gaps
--DESCRIPTION--
module: standard.arrays
generated timestamp: 20260626T000000Z
generator version: phpt-standard-arrays-v1
reason: substr_replace walks the replacement array's values in iteration order, so an unset() gap is skipped rather than producing an empty replacement (tests/standard/strings/substr_replace_array_unset.phpt)
--FILE--
<?php
$replacement = ['A', 'C', 'B'];
unset($replacement[1]);
print_r(substr_replace(['1 string', '2 string'], $replacement, 0));
?>
--EXPECT--
Array
(
    [0] => A
    [1] => B
)

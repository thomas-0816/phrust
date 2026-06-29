--TEST--
Generated arrays.references: key normalization, append keys, and unset holes
--DESCRIPTION--
module: arrays.references
generated timestamp: 20260628T000000Z
generator version: phpt-arrays-references-v1
reason: focused coverage for ordered array keys, numeric-string key normalization, append next-key tracking, and unset holes from Reference PHP output
--FILE--
<?php
$a = [];
$a["8"] = "eight";
$a["08"] = "zero-eight";
$a[true] = "true";
$a[false] = "false";
$a[-1] = "neg";
$a[] = "append";
unset($a[8]);
$a[] = "after";

foreach ($a as $k => $v) {
    var_dump($k, $v);
}
?>
--EXPECT--
string(2) "08"
string(10) "zero-eight"
int(1)
string(4) "true"
int(0)
string(5) "false"
int(-1)
string(3) "neg"
int(9)
string(6) "append"
int(10)
string(5) "after"

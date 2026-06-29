--TEST--
Generated arrays.references: copy-on-write separation for nested arrays and element references
--DESCRIPTION--
module: arrays.references
generated timestamp: 20260628T000000Z
generator version: phpt-arrays-references-v1
reason: focused coverage for by-value array assignment sharing until scalar and nested writes separate from Reference PHP output
--FILE--
<?php
$a = ["x" => 1, "nested" => ["y" => 2]];
$b = $a;
$b["x"] = 3;
$b["nested"]["y"] = 4;
echo $a["x"], ":", $a["nested"]["y"], "|", $b["x"], ":", $b["nested"]["y"], "\n";

$r = 9;
$b["x"] =& $r;
$r = 5;
echo $a["x"], "|", $b["x"], "|", $r, "\n";
?>
--EXPECT--
1:2|3:4
1|5|5

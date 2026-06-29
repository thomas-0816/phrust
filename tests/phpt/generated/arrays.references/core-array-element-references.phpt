--TEST--
Generated arrays.references: array element references and unset cells
--DESCRIPTION--
module: arrays.references
generated timestamp: 20260628T000000Z
generator version: phpt-arrays-references-v1
reason: focused coverage for array element reference aliases, unset preserving the reference cell, and append-by-reference from Reference PHP output
--FILE--
<?php
$a = ["x" => 1, "y" => 2];
$r =& $a["x"];
unset($a["x"]);
$r = 7;

echo isset($a["x"]) ? "bad" : "unset";
echo "|", $a["y"], "|", $r, "\n";

$b = [];
$b[] =& $r;
$r = 8;
echo $b[0], "|", $r, "\n";
?>
--EXPECT--
unset|2|7
8|8

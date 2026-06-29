--TEST--
Generated arrays.references: isset and empty over local array dimensions
--DESCRIPTION--
module: arrays.references
generated timestamp: 20260628T000000Z
generator version: phpt-arrays-references-v1
reason: focused coverage for isset() and empty() on present, null, missing, and nested array dimensions from Reference PHP output
--FILE--
<?php
$a = ["x" => 0, "n" => null, "nested" => ["y" => "0", "z" => ""]];

echo isset($a["x"]) ? "T" : "F";
echo isset($a["n"]) ? "T" : "F";
echo isset($a["missing"]) ? "T" : "F";
echo "|";
echo empty($a["x"]) ? "T" : "F";
echo empty($a["nested"]["y"]) ? "T" : "F";
echo empty($a["nested"]["z"]) ? "T" : "F";
echo empty($a["missing"]["child"]) ? "T" : "F";
echo "\n";
?>
--EXPECT--
TFF|TTTT

--TEST--
Generated arrays.references: foreach by-reference over a local array
--DESCRIPTION--
module: arrays.references
generated timestamp: 20260628T000000Z
generator version: phpt-arrays-references-v1
reason: focused coverage for local-array by-reference foreach, appended entries, value mutation, and the lingering loop reference from Reference PHP output
--FILE--
<?php
$items = [1, 2];
$done = false;

foreach ($items as $k => &$v) {
    echo $k, ":", $v, ";";
    $v += 10;
    if (!$done) {
        $items[] = 3;
        $done = true;
    }
}

$v = 99;
echo "|";

foreach ($items as $k => $v2) {
    echo $k, ":", $v2, ";";
}

echo "\n";
?>
--EXPECT--
0:1;1:2;2:3;|0:11;1:12;2:99;

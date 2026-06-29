--TEST--
Generated arrays.references: foreach by-value snapshots local array iteration
--DESCRIPTION--
module: arrays.references
generated timestamp: 20260628T000000Z
generator version: phpt-arrays-references-v1
reason: focused coverage for by-value foreach using an iteration snapshot while the source array mutates from Reference PHP output
--FILE--
<?php
$items = [1, 2];

foreach ($items as $k => $v) {
    echo $k, ":", $v, ";";
    $items[] = 9;
    unset($items[0]);
}

echo "|";

foreach ($items as $k => $v) {
    echo $k, ":", $v, ";";
}

echo "\n";
?>
--EXPECT--
0:1;1:2;|1:2;2:9;3:9;

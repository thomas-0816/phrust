<?php
// runtime-semantics: category=foreach expect=pass php_ref_required=1
// Foreach by value iterates the array as it was when the loop started:
// appends, overwrites, and unsets inside the loop must not change what
// this iteration visits, and the array keeps the mutations afterwards.
$items = [10, 20, 30];
$log = [];
foreach ($items as $i => $v) {
    $log[] = "$i=$v";
    if ($i === 0) {
        $items[] = 40;          // append: not visited by this loop
        $items[2] = 99;         // overwrite ahead: original value visited
        unset($items[1]);       // unset ahead: still visited
    }
}
echo implode(",", $log), "\n";
echo implode(",", $items), "\n";

// Nested foreach over the same array: each loop gets its own snapshot.
$grid = ["a" => 1, "b" => 2];
$pairs = [];
foreach ($grid as $k1 => $v1) {
    foreach ($grid as $k2 => $v2) {
        $pairs[] = "$k1$k2=" . ($v1 + $v2);
        $grid["c"] = 3;
    }
}
echo implode(",", $pairs), "\n";
echo count($grid), "\n";

// Reassigning the iterated variable inside the loop does not disturb
// the iteration source.
$src = [1, 2, 3];
$sum = 0;
foreach ($src as $v) {
    $sum += $v;
    $src = [];
}
echo $sum, "|", count($src), "\n";

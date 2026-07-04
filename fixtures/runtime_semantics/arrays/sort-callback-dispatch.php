<?php
// runtime-semantics: category=arrays expect=pass php_ref_required=1
// Sort comparator dispatch: string function names, closures, builtin
// comparators, bool/int/string returns, throwing comparators, external
// state mutation, uasort/uksort key preservation, invalid callbacks.
function by_len($a, $b) {
    return strlen($a) <=> strlen($b);
}
$words = ["pear", "fig", "banana", "kiwi"];
usort($words, 'by_len');
echo implode(",", $words), "\n";

$nums = [5, 1, 4, 2];
usort($nums, fn($a, $b) => $b - $a);
echo implode(",", $nums), "\n";

$strs = ["b", "a", "c"];
usort($strs, 'strcmp');
echo implode(",", $strs), "\n";

$calls = 0;
$tracked = [3, 1, 2];
usort($tracked, function ($a, $b) use (&$calls) {
    $calls++;
    return $a <=> $b;
});
echo implode(",", $tracked), "|", ($calls > 0 ? "counted" : "missed"), "\n";

$assoc = ["b" => 2, "a" => 1, "c" => 3];
uasort($assoc, 'by_value_desc');
function by_value_desc($x, $y) {
    return $y <=> $x;
}
foreach ($assoc as $k => $v) {
    echo "$k=$v,";
}
echo "\n";

uksort($assoc, 'strcmp');
echo implode(",", array_keys($assoc)), "\n";

function stringy_cmp($a, $b) {
    return ($a < $b) ? "-1" : (($a > $b) ? "1" : "0");
}
$s2 = [3, 1, 2];
usort($s2, 'stringy_cmp');
echo implode(",", $s2), "\n";

try {
    $boom = [2, 1];
    usort($boom, function ($a, $b) {
        throw new RuntimeException("cmp-fail");
    });
} catch (RuntimeException $e) {
    echo $e->getMessage(), "\n";
}

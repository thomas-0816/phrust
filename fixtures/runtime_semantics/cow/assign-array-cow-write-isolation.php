<?php
// runtime-semantics: category=cow expect=pass php_ref_required=1
// Array assignment shares storage until the first write; writes through
// either variable must not leak into the other, including nested arrays
// and writes performed inside a function on a by-value parameter.
$a = [1, 2, ["x" => 10]];
$b = $a;
$b[0] = 99;
$b[2]["x"] = 77;
echo $a[0], "|", $b[0], "|", $a[2]["x"], "|", $b[2]["x"], "\n";

function mutate($arr) {
    $arr[] = "added";
    $arr[1] = "swapped";
    return count($arr);
}

$c = ["keep", "me"];
$grew = mutate($c);
echo $grew, "|", count($c), "|", $c[1], "\n";

$d = $a;
$a[] = 4;
echo count($a), "|", count($d), "\n";

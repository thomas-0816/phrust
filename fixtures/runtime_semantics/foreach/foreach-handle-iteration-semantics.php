<?php
// runtime-semantics: category=foreach expect=pass php_ref_required=1
// Handle-based by-value foreach: source mutation isolation, loop-variable
// writes, reference elements, by-reference foreach, and nested loops.
$src = [1, 2, 3, 4];
$sum = 0;
foreach ($src as $i => $v) {
    $sum += $v;
    if ($i === 1) {
        $src[] = 99;         // appended after iteration started: not visited
        $src[0] = 1000;      // COW separates; this loop keeps original values
    }
    $v = -1;                 // loop variable write never touches the array
}
echo $sum, "|", implode(",", $src), "\n";

$r = 5;
$withRef = [1, &$r, 3];
$seen = [];
foreach ($withRef as $v) {
    $seen[] = $v;
    $r = 50;                 // reference element visited later
}
echo implode(",", $seen), "|", $r, "\n";

$byRef = [1, 2, 3];
foreach ($byRef as &$v) {
    $v *= 10;
}
unset($v);
echo implode(",", $byRef), "\n";

$grid = ["a" => 1, "b" => 2];
$out = "";
foreach ($grid as $k1 => $v1) {
    foreach ($grid as $k2 => $v2) {
        $out .= "$k1$k2" . ($v1 + $v2) . ";";
    }
}
echo $out, "\n";

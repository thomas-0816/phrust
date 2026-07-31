<?php
function native_scalar_sort_boundary(): array {
    $scalars = [3.5, -12, true, false, null, "2", "10"];
    $scalarResult = sort($scalars, SORT_STRING);

    $folded = [7 => "Zulu", 2 => "alpha", 9 => "Beta", 4 => "ALPHA"];
    $foldedResult = asort($folded, SORT_STRING | SORT_FLAG_CASE);

    $natural = [8 => "img12", 3 => "Img2", 5 => "img1", 1 => "IMG02"];
    $naturalResult = natcasesort($natural);

    $keys = ["B" => 1, "a" => 2, "C" => 3, "A" => 4];
    $keyResult = ksort($keys, SORT_STRING | SORT_FLAG_CASE);

    return [
        $scalarResult,
        $scalars,
        $foldedResult,
        $folded,
        $naturalResult,
        $natural,
        strnatcasecmp("img1", "IMG02"),
        strnatcasecmp("Img2", "IMG02"),
        $keyResult,
        $keys,
    ];
}

for ($warm = 0; $warm < 64; $warm++) {
    native_scalar_sort_boundary();
}

var_dump(native_scalar_sort_boundary());

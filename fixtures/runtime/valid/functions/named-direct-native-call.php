<?php
// runtime-fixture: kind=valid expected_stdout="5|20|7\n32|5\n5|6|7\n"

function named_direct_native_target(&$first, $second = 20, $third = 30) {
    $first += 1;
    echo $first, "|", $second, "|", $third, "\n";
    return $first + $second + $third;
}

$value = 4;
echo named_direct_native_target(third: 7, first: $value), "|", $value, "\n";

function direct_native_unpack_target($first, $second, $third) {
    echo $first, "|", $second, "|", $third, "\n";
}

$tail = [6, 7];
direct_native_unpack_target(5, ...$tail);

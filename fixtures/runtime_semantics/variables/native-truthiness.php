<?php
function native_truthiness(mixed $value): array {
    return [(bool) $value, !$value, empty($value)];
}

foreach ([
    null,
    false,
    true,
    0,
    1,
    0.0,
    -0.0,
    1.5,
    "",
    "0",
    "00",
    [],
    [0],
    new stdClass(),
] as $value) {
    var_dump(native_truthiness($value));
}

$value = "0";
$reference =& $value;
var_dump(native_truthiness($reference));

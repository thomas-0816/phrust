<?php

function native_authoritative_truthiness(&$value): string
{
    return $value ? '1' : '0';
}

$warm = 1;
for ($i = 0; $i < 10; $i++) {
    native_authoritative_truthiness($warm);
}

$values = [
    null,
    false,
    true,
    0,
    -2,
    0.0,
    -0.0,
    1.5,
    '',
    '0',
    '00',
    [],
    [1],
    new stdClass(),
];

foreach ($values as &$value) {
    echo native_authoritative_truthiness($value);
}
echo "\n";

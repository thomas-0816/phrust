<?php

const NATIVE_NAMED_PAYLOAD = [
    '07' => 'leading-zero',
    7 => 'integer',
    'tail',
];

class NativeConstantPayloadOwner
{
    public const PAYLOAD = [
        '7' => 'numeric-string',
        false => 'false-key',
        2.9 => 'float-key',
    ];
}

require __DIR__ . '/native_cross_unit_constant_values_target.php';

$result = native_receive_constant_values(
    NATIVE_NAMED_PAYLOAD,
    NativeConstantPayloadOwner::PAYLOAD,
);

echo $result[0]['07'], "\n";
echo $result[0][7], "\n";
echo $result[0][8], "\n";
echo $result[1][7], "\n";
echo $result[1][0], "\n";
echo $result[1][2], "\n";

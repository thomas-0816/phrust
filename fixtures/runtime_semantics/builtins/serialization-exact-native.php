<?php
// runtime-semantics: category=builtins expect=pass

function run_native_serialization(string $binary): array
{
    $value = [
        null,
        false,
        true,
        -42,
        1.5,
        'plain',
        'binary' => $binary,
        'nested' => ['key' => 'value', 7],
    ];
    $encoded = serialize($value);
    $decoded = unserialize($encoded);

    return [
        $encoded,
        serialize($decoded),
        bin2hex($decoded['binary']),
        $decoded['nested']['key'],
        $decoded['nested'][0],
    ];
}

$binary = hex2bin('00ff6e6174697665');
$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = run_native_serialization($binary);
}

var_dump($result);

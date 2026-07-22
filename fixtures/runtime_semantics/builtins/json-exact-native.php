<?php

function run_exact_json(): array
{
    $encoded = json_encode(['name' => 'Phrust', 'values' => [1, 2, 3]]);
    $decoded = json_decode('{"enabled":true,"count":2}', true);
    $valid = json_validate('{"ok":[1,2]}');
    $invalid = json_decode('{');
    $lastError = json_last_error();
    $lastMessage = json_last_error_msg();

    return [
        $encoded,
        $decoded,
        $valid,
        $invalid,
        $lastError,
        $lastMessage,
    ];
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = run_exact_json();
}
var_dump($result);

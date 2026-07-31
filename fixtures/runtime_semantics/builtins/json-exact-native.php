<?php

function run_exact_json(): array
{
    $encoded = json_encode(['name' => 'Phrust', 'values' => [1, 2, 3]]);
    $decoded = json_decode(
        '{"enabled":true,"count":2,"items":[1,2,3,4,5,6],"nested":{"x":1,"x":2},"empty":{}}',
        true,
    );
    $valid = json_validate('{"ok":[1,2]}');
    $invalid = json_decode('{');
    $lastError = json_last_error();
    $lastMessage = json_last_error_msg();
    $pretty = json_encode(
        ['path' => '/native', 'unicode' => 'Größe', 'float' => 1.0],
        JSON_PRETTY_PRINT
            | JSON_UNESCAPED_UNICODE
            | JSON_UNESCAPED_SLASHES
            | JSON_PRESERVE_ZERO_FRACTION,
    );
    $numeric = json_encode(['12.5', '001', 1.0], JSON_NUMERIC_CHECK | JSON_PRESERVE_ZERO_FRACTION);
    $escaped = json_encode(
        ['<tag>', 'a&b', "'quoted'", '"double"'],
        JSON_HEX_TAG | JSON_HEX_AMP | JSON_HEX_APOS | JSON_HEX_QUOT,
    );
    $substituted = json_encode(
        hex2bin('6e6174697665ff6a736f6e'),
        JSON_INVALID_UTF8_SUBSTITUTE,
    );

    return [
        $encoded,
        $decoded,
        $valid,
        $invalid,
        $lastError,
        $lastMessage,
        $pretty,
        $numeric,
        $escaped,
        $substituted,
    ];
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = run_exact_json();
}
var_dump($result);

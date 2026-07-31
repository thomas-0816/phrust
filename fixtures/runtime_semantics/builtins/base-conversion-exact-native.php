<?php
// runtime-semantics: category=builtins expect=pass

function run_native_base_conversions(string $large): array
{
    return [
        decbin(0),
        decbin(-1),
        dechex(-1),
        decoct(-1),
        base_convert('ff', 16, 2),
        base_convert('101010', 2, 36),
        base_convert($large, 36, 2),
    ];
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = run_native_base_conversions('zzzzzzzzzzzzzzzzzzzz');
}

var_dump($result);

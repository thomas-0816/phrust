<?php
// runtime-semantics: category=builtins expect=pass

function run_native_http_query(array $data): array
{
    return [
        http_build_query($data),
        http_build_query($data, 'n_', ';'),
        http_build_query($data, 'n_', '&', PHP_QUERY_RFC3986),
    ];
}

$data = [
    0 => 'zero value',
    'space key' => 'a b+c/~',
    'nested' => [
        'x/y' => 42,
        2 => true,
        'false' => false,
        'null' => null,
        'float' => 1.25,
    ],
    'tail' => 'done',
];

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = run_native_http_query($data);
}

var_dump($result);

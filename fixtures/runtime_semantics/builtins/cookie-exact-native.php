<?php
// runtime-semantics: category=builtins expect=pass

function run_exact_cookie(): array
{
    return [
        setcookie('native', 'a b', 0, '/', '', true, true),
        setrawcookie('raw', 'a-b', [
            'path' => '/api',
            'secure' => true,
            'samesite' => 'Lax',
        ]),
    ];
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = run_exact_cookie();
}
var_dump($result);

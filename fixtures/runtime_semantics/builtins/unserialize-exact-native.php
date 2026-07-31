<?php

function compile_exact_unserialize(string $payload): mixed
{
    return unserialize($payload);
}

$nested = compile_exact_unserialize(
    'a:3:{s:1:"x";i:1;s:1:"y";a:1:{i:0;s:3:"old";}s:1:"x";a:2:{i:0;s:3:"new";i:0;s:5:"final";}}'
);

var_dump([
    compile_exact_unserialize('N;'),
    compile_exact_unserialize('b:1;'),
    compile_exact_unserialize('i:-42;'),
    compile_exact_unserialize('d:1.5;'),
    compile_exact_unserialize('s:5:"value";'),
    $nested,
]);

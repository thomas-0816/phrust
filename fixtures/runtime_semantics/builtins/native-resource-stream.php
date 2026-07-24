<?php

function nativeResourceStream(string $payload): array
{
    $stream = fopen('php://memory', 'w+');
    $alias = $stream;

    $prefix = fwrite($alias, $payload, 6);
    $suffix = fwrite($stream, '-tail');
    $closed = fclose($alias);

    return [$prefix, $suffix, $closed];
}

var_dump(nativeResourceStream('native-resource'));

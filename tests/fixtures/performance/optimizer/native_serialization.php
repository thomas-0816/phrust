<?php

function encode_native_array(array $value): string
{
    return serialize($value);
}

function decode_native_value(string $wire)
{
    return unserialize($wire);
}

$value = [
    'plain' => [1, true, null, 3.5],
    7 => "x\0y",
    '08' => ['nested' => -4],
];

$wire = encode_native_array($value);
echo str_replace("\0", "\\0", $wire), "\n";
echo str_replace("\0", "\\0", encode_native_array(decode_native_value($wire))), "\n";

$duplicates = decode_native_value('a:3:{s:1:"x";i:1;s:1:"x";i:2;s:2:"08";s:4:"kept";}');
echo encode_native_array($duplicates), "\n";

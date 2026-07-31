<?php

function object_identity_probe(object $value): string
{
    $id = spl_object_id($value);
    $hash = spl_object_hash($value);

    return ($id > 0 ? 'positive' : 'invalid')
        . ':' . ($id === spl_object_id($value) ? 'stable-id' : 'changed-id')
        . ':' . ($hash === spl_object_hash($value) ? 'stable-hash' : 'changed-hash')
        . ':' . (strlen($hash) === 32 ? 'hash-32' : 'bad-hash');
}

$object = new stdClass();
$closure = static function (): void {};

echo object_identity_probe($object), "\n";
echo object_identity_probe($closure), "\n";
echo spl_object_id($object) !== spl_object_id($closure) ? "distinct\n" : "collision\n";

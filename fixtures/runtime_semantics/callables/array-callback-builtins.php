<?php

function add_marker(array $value): array
{
    $value['marked'] = true;
    return $value;
}

$values = array(
    'first' => array('enabled' => true),
    'second' => array('enabled' => false),
);

$mapped = array_map('add_marker', $values);
$filtered = array_filter(
    $mapped,
    static function (array $value): bool {
        return $value['enabled'];
    }
);

var_dump($mapped);
var_dump($filtered);

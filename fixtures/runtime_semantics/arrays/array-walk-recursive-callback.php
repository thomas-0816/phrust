<?php
// runtime-semantics: category=arrays expect=pass

$values = [
    'top' => 'a',
    'nested' => ['left' => 'b', 'deep' => ['right' => 'c']],
];

array_walk_recursive(
    $values,
    static function (&$value, $key, $prefix): void {
        $value = $prefix . $key . ':' . $value;
    },
    'x-'
);

var_dump($values);

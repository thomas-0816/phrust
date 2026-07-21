<?php

$array = [0 => null, 1 => 'one', '' => 'empty', 'name' => 3, '01' => 4];

foreach ([0, 1, '1', '', 'name', 'missing', '01'] as $key) {
    echo json_encode($key), '=', array_key_exists($key, $array) ? "yes\n" : "no\n";
}

$key = 'name';
$reference =& $key;
echo 'reference=', array_key_exists($reference, $array) ? "yes\n" : "no\n";

echo 'null=', array_key_exists(null, $array) ? "yes\n" : "no\n";
echo 'float=', array_key_exists(1.5, $array) ? "yes\n" : "no\n";

try {
    array_key_exists([], $array);
} catch (Throwable $error) {
    echo get_class($error), ':', $error->getMessage(), "\n";
}

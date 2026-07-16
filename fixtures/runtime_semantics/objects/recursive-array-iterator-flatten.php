<?php

$iterator = new RecursiveIteratorIterator(
    new RecursiveArrayIterator(['a', ['b', 'c'], 'nested' => ['d']]),
);

foreach ($iterator as $key => $value) {
    var_dump($key, $value);
}

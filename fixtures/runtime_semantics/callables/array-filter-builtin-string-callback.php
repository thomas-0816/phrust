<?php

$key_sets = array(
    array(0, 1, 2),
    array('first', 1, 'second'),
    array_keys(array('name' => 'value', 4 => 'four')),
);

foreach ($key_sets as $keys) {
    var_dump(array_filter($keys, 'is_string'));
    var_dump(array_filter($keys, 'is_int'));
}

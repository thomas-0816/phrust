<?php

function native_callback_reference_increment(int &$value): int
{
    $value += 3;
    return $value;
}

$value = 4;
$arguments = [&$value];
$result = call_user_func_array('native_callback_reference_increment', $arguments);

echo $result, ':', $value, ':', $arguments[0], "\n";

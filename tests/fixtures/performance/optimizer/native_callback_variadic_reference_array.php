<?php

function native_callback_variadic_reference_increment(int &...$values): int
{
    $values[0] += 2;
    $values[1] += 3;
    return $values[0] + $values[1];
}

function native_callback_variadic_reference_invoke(int &$left, int &$right): int
{
    $arguments = [&$left, &$right];
    return call_user_func_array('native_callback_variadic_reference_increment', $arguments);
}

for ($iteration = 0; $iteration < 32; $iteration++) {
    $left = 1;
    $right = 2;
    $result = native_callback_variadic_reference_invoke($left, $right);
}

echo $result, ':', $left, ':', $right, "\n";

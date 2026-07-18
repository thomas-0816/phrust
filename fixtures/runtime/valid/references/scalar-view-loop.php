<?php
function scalar_view_loop(&$value)
{
    $sum = 0;
    for ($index = 0; $index < 1000; $index++) {
        $sum += $value;
    }
    $value = 2;
    for ($index = 0; $index < 1000; $index++) {
        $sum += $value;
    }
    return $sum;
}

$value = 1;
echo scalar_view_loop($value), ':', $value, "\n";

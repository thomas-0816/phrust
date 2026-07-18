<?php
function array_operand_loop(&$items)
{
    $sum = 0;
    for ($index = 0; $index < 1000; $index++) {
        $sum += $items['value'];
    }
    return $sum;
}

$items = array('value' => 3);
echo array_operand_loop($items), "\n";

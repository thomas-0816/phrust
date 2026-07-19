<?php
function array_operand_loop(&$items)
{
    $sum = 0;
    for ($index = 0; $index < 1000; $index++) {
        $sum += $items['value'];
    }
    return $sum;
}

function mark_array_reference_later(&$items)
{
}

function plain_array_operand_loop($items)
{
    if (!isset($items['value']) || empty($items['value'])) {
        return -1;
    }
    $sum = 0;
    for ($index = 0; $index < 1000; $index++) {
        $sum += $items['value'];
    }
    mark_array_reference_later($items);
    return $sum;
}

$items = array('value' => 3);
echo array_operand_loop($items), "\n";
echo plain_array_operand_loop(array('value' => 3)), "\n";

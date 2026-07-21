<?php
function cross_unit_transfer_values($array, $object, $text, $number) {
    $array['inside'] = 2;
    return array($array, $object, $text, $number + 1, 'unit-literal');
}

function cross_unit_increment(&$value) {
    $value++;
}

function cross_unit_named_increment(&$first, $second) {
    $first += $second;
}

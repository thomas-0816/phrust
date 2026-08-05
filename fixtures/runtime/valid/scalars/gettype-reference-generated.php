<?php
class TypeMarker {}

function classify_type(&$value): string {
    return gettype($value) . '|' . get_debug_type($value);
}

$array = [1];
$object = new TypeMarker();
$integer = 7;
echo classify_type($array), "\n";
echo classify_type($object), "\n";
echo classify_type($integer), "\n";

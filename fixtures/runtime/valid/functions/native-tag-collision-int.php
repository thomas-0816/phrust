<?php
function preserve_tag_collision_int(int $value): int
{
    return $value;
}

function store_tag_collision_int(&$target, int $value): void
{
    $target = $value;
}

$decoded = json_decode('9219149912204115968');
$sum = json_decode('9219149912204115967') + 1;
$inverted = ~json_decode('-9219149912204115969');
$values = [$sum => 'kept'];
$referenced = 0;
$alias =& $referenced;
$alias = $sum;
$call_referenced = 0;
store_tag_collision_int($call_referenced, $sum);

echo gettype($decoded), "\n";
echo $decoded, "\n";
echo $sum, "\n";
echo $inverted, "\n";
echo preserve_tag_collision_int($sum), "\n";
echo preserve_tag_collision_int(9219149912204115968), "\n";
echo +9219149912204115968, "\n";
echo $values[$sum], "\n";
echo $referenced, "\n";
echo $call_referenced, "\n";

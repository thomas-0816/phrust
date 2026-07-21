<?php

function mutate_array_argument(array $value): array
{
    $value['top'] = 2;
    $value['nested']['inner'] = 4;
    return $value;
}

function fetch_nested_array(array $value): array
{
    return $value['nested'];
}

class NativeArrayBox
{
    public array $value;
}

$original = ['top' => 1, 'nested' => ['inner' => 3]];
$mutated = mutate_array_argument($original);
echo $original['top'], '|', $mutated['top'], '|';
echo $original['nested']['inner'], '|', $mutated['nested']['inner'], "\n";

$nested = fetch_nested_array($original);
$nested['inner'] = 5;
echo $original['nested']['inner'], '|', $nested['inner'], "\n";

$box = new NativeArrayBox();
$box->value = $original;
$property = $box->value;
$property['top'] = 6;
echo $box->value['top'], '|', $property['top'], "\n";

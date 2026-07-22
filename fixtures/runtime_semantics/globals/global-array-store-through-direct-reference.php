<?php
// runtime-semantics: category=globals expect=pass

global $allowed;
$allowed = [
    'a' => ['local' => true],
    'b' => ['local' => false],
];

function add_global_flag(array $attributes): array
{
    $attributes['global'] = true;
    return $attributes;
}

$mapped = array_map('add_global_flag', $allowed);
echo count($mapped), ':', (int) $mapped['a']['global'], ':', (int) $mapped['b']['local'], "\n";

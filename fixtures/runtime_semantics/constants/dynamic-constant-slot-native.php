<?php

function dynamic_constant_slot_native(bool $read): string
{
    return $read ? DYNAMIC_CONSTANT_SLOT_NATIVE : 'warm';
}

for ($iteration = 0; $iteration < 32; $iteration++) {
    dynamic_constant_slot_native(false);
}

define('DYNAMIC_CONSTANT_SLOT_NATIVE', 'published');

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = dynamic_constant_slot_native(true);
}

var_dump(
    $result,
    defined('DYNAMIC_CONSTANT_SLOT_NATIVE'),
    constant('DYNAMIC_CONSTANT_SLOT_NATIVE'),
);

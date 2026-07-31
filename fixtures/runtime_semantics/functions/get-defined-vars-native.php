<?php

function get_defined_vars_native(array $input): array
{
    $copy = $input;
    $shared = ['value' => 1];
    $alias =& $shared;
    $gone = 'unset';
    unset($gone);

    return get_defined_vars();
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = get_defined_vars_native(['input' => $iteration]);
}

$result['alias']['value'] = 7;
var_dump($result);

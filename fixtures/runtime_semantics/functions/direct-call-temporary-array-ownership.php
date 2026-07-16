<?php

function read_direct_call_option($value, array $options): string
{
    return $value . ':' . $options['suffix'];
}

$value = 'stable';

echo read_direct_call_option(
    $value,
    array('suffix' => 'temporary'),
), "\n";
echo $value, "\n";

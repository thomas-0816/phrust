<?php

function append_a($value)
{
    return $value . "a";
}

function append_b($value)
{
    return $value . "b";
}

$callbacks = ["append_a", "append_b"];
$value = "x";
foreach ($callbacks as $callback) {
    $value = $callback($value);
}

echo $value, "\n";

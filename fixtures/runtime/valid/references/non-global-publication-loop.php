<?php
function increment_reference(&$value)
{
    $value++;
}

function reference_publication_driver()
{
    $value = 0;
    for ($index = 0; $index < 1000; $index++) {
        increment_reference($value);
    }
    return $value;
}

echo reference_publication_driver(), "\n";

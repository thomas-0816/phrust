<?php
// runtime-fixture: kind=valid expected_stdout="11|2\n"

function optimizer_direct_array_reference_write(): void
{
    $value = ['x' => 10];
    $reference =& $value;
    $reference['x'] = 11;
    $reference[] = 12;
    echo $value['x'], '|', count($value), "\n";
}

optimizer_direct_array_reference_write();

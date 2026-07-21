<?php
// runtime-fixture: kind=valid expected_stdout="20|21\n"

function optimizer_direct_array_undefined_write(): void
{
    $undefined['x'] = 20;
    $appended[] = 21;
    echo $undefined['x'], '|', $appended[0], "\n";
}

optimizer_direct_array_undefined_write();

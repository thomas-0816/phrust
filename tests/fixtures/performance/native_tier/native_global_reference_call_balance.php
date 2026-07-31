<?php
function native_global_reference_call_balance_flag() {
    return false;
}

function native_global_reference_call_balance_id() {
    return 1;
}

require __DIR__ . '/native_global_reference_call_balance_target.php';
native_global_reference_call_balance_init();

$sum = 0;
for ($index = 0; $index < 100; $index++) {
    $sum += native_global_reference_call_balance_value();
}

echo $sum, "\n";

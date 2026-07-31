<?php
require __DIR__ . '/native_global_reference_call_balance_class.php';

function native_global_reference_call_balance_init() {
    $GLOBALS['native_global_reference_call_balance'] = new NativeGlobalReferenceCallBalance();
}

function native_global_reference_call_balance_value() {
    global $native_global_reference_call_balance;
    return $native_global_reference_call_balance->value();
}

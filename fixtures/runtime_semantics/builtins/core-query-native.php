<?php
// runtime-semantics: category=builtins expect=pass

const NATIVE_CORE_QUERY_BASE = 17;
const NATIVE_CORE_QUERY_DECLARED = ['declared' => NATIVE_CORE_QUERY_BASE];
define('NATIVE_CORE_QUERY_DYNAMIC', 23);

function native_core_queries(
    string $declared,
    string $dynamic,
    string $standard,
): array {
    $flat = ini_get_all('core', false);
    $nullDetails = ini_get_all('core', null);
    $details = ini_get_all('core');
    return [
        constant($declared),
        constant($dynamic),
        constant($standard) === 8,
        sizeof([1, 2, 3]),
        $flat['display_errors'] === $details['display_errors']['local_value'],
        $nullDetails['display_errors']
            === $details['display_errors']['local_value'],
        $details['display_errors']['global_value']
            === $details['display_errors']['local_value'],
        $details['display_errors']['access'] === 7,
    ];
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = native_core_queries(
        'NATIVE_CORE_QUERY_DECLARED',
        'NATIVE_CORE_QUERY_DYNAMIC',
        'PHP_INT_SIZE',
    );
}
var_dump($result);

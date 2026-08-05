<?php
unset($GLOBALS['generic_direct_reference_payload']);

function generic_direct_reference_payload_value() {
    global $generic_direct_reference_payload;
    return $generic_direct_reference_payload;
}

echo is_null(generic_direct_reference_payload_value()) ? 'unset' : 'stale', '|';
$generic_direct_reference_payload = 'native';
echo generic_direct_reference_payload_value(), '|';
unset($GLOBALS['generic_direct_reference_payload']);
echo is_null(generic_direct_reference_payload_value()) ? 'unset' : 'stale', "\n";

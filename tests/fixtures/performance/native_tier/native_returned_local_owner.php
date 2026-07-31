<?php

function perf_native_returned_local_owner(string $json): array
{
    $decoded = json_decode($json, true);
    return $decoded;
}

function perf_native_reference_alias_result(array $args, array $defaults): array
{
    $parsed =& $args;
    if ($defaults) {
        return array_merge($defaults, $parsed);
    }
    return $parsed;
}

function perf_native_large_frame_cleanup(): int
{
    $owner00 = [];
    $owner01 = [];
    $owner02 = [];
    $owner03 = [];
    $owner04 = [];
    $owner05 = [];
    $owner06 = [];
    $owner07 = [];
    $owner08 = [];
    $owner09 = [];
    $owner10 = [];
    $owner11 = [];
    $owner12 = [];
    $owner13 = [];
    $owner14 = [];
    $owner15 = [];
    $owner16 = [];
    $owner17 = [];
    $owner18 = [];
    $owner19 = [];
    $owner20 = [];
    $owner21 = [];
    $owner22 = [];
    $owner23 = [];
    $owner24 = [];
    $owner25 = [];
    $owner26 = [];
    $owner27 = [];
    $owner28 = [];
    $owner29 = [];
    $owner30 = [];
    $owner31 = [];
    $owner32 = [];
    $owner33 = [];
    $owner34 = [];
    $owner35 = [];
    $owner36 = [];
    $owner37 = [];
    $owner38 = [];
    $owner39 = [];
    return 1;
}

$sum = 0;
for ($i = 0; $i < 100; $i++) {
    $sum += perf_native_returned_local_owner('{"value":7}')['value'];
    $sum += perf_native_reference_alias_result(
        array('value' => 3),
        array('fallback' => 1),
    )['value'];
    $sum += perf_native_reference_alias_result(
        array('value' => 2),
        array(),
    )['value'];
}
$sum += perf_native_large_frame_cleanup();
echo $sum, "\n";

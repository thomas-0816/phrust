<?php

$context = stream_context_create([
    'http' => [
        'method' => 'GET',
        'timeout' => 3,
    ],
]);
$initial = stream_context_get_options($context);

$setOne = stream_context_set_option($context, 'http', 'method', 'POST');
$setMany = stream_context_set_options($context, [
    'http' => ['follow_location' => 0],
    'ssl' => ['verify_peer' => false],
]);
$updated = stream_context_get_options($context);

$marker = new stdClass();
$marker->id = 7;
$setCold = stream_context_set_option($context, 'custom', 'marker', $marker);
$afterCold = stream_context_get_options($context);

$defaultResource = stream_context_set_default([
    'http' => ['method' => 'HEAD'],
]);
$defaultSnapshot = stream_context_get_options(stream_context_get_default());

echo json_encode([
    'initial' => $initial,
    'set' => [$setOne, $setMany, $setCold],
    'updated' => $updated,
    'cold' => $afterCold,
    'defaultResource' => get_resource_type($defaultResource),
    'default' => $defaultSnapshot,
]), "\n";

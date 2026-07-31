<?php

function inspectResourceQueries($resource): array
{
    $id = get_resource_id($resource);
    $type = get_resource_type($resource);
    $all = get_resources();
    $streams = get_resources('stream');

    $typeError = false;
    try {
        get_resource_id(null);
    } catch (TypeError $error) {
        $typeError = true;
    }

    $filterTypeError = false;
    try {
        get_resources([]);
    } catch (TypeError $error) {
        $filterTypeError = true;
    }

    $valueError = false;
    try {
        get_resources('not-a-resource-type');
    } catch (ValueError $error) {
        $valueError = true;
    }

    return [
        is_int($id) && $id > 0,
        $type === 'stream',
        isset($all[$id]) && get_resource_id($all[$id]) === $id,
        isset($streams[$id]) && get_resource_type($streams[$id]) === 'stream',
        $typeError,
        $filterTypeError,
        $valueError,
    ];
}

$resource = fopen('php://memory', 'w+');
foreach (inspectResourceQueries($resource) as $result) {
    var_dump($result);
}
fclose($resource);

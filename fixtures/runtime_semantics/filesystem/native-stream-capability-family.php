<?php

$payload = 'native-stream-filter';

$stream = fopen('php://memory', 'w+');
fwrite($stream, $payload);
rewind($stream);
$filter = stream_filter_append($stream, 'string.rot13', STREAM_FILTER_READ);
$filterType = get_resource_type($filter);
$decoded = stream_get_contents($stream);
$removed = stream_filter_remove($filter);
$tty = stream_isatty($stream);
$timeout = stream_set_timeout($stream, 1, 2);
fclose($stream);

$second = fopen('php://memory', 'w+');
fwrite($second, 'mixed');
rewind($second);
$prepended = stream_filter_prepend($second, 'string.toupper', STREAM_FILTER_READ);
$prependType = get_resource_type($prepended);
$upper = stream_get_contents($second);
$prependRemoved = stream_filter_remove($prepended);
fclose($second);

echo json_encode([
    'filterType' => $filterType,
    'decoded' => $decoded,
    'removed' => $removed,
    'tty' => $tty,
    'timeout' => $timeout,
    'prependType' => $prependType,
    'upper' => $upper,
    'prependRemoved' => $prependRemoved,
]), "\n";

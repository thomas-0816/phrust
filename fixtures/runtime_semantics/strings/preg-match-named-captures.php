<?php

$matches = [];
$result = preg_match(
    '#^(?P<host>[^:/]*)(?::(?P<port>[\d]+))?#',
    '127.0.0.1:33306',
    $matches,
);

var_dump($result, $matches['host'], $matches['port']);

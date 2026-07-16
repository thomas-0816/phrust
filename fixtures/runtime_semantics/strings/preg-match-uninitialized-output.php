<?php

$result = preg_match(
    '/^((?P<scheme>[^:\/?#]+):)?(\/\/(?P<authority>[^\/?#]*))?(?P<path>[^?#]*)$/',
    'https://example.test/path',
    $matches,
);

var_dump(
    $result,
    $matches[1],
    $matches['scheme'],
    $matches['authority'],
    $matches['path'],
);

<?php

require __DIR__ . '/_data/preg-match-child.php';

[$result, $matches] = Fixture\Preg\ExternalPregMatchFixture::parse('https://example.test/path');

var_dump(
    $result,
    $matches[1],
    $matches['scheme'],
    $matches['authority'],
    $matches['path'],
);

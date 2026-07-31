<?php

function parse_url_exact_native(): array
{
    $url = 'https://user:pass@example.com:8443/a/b?x=1#fragment';

    return [
        parse_url($url),
        parse_url($url, PHP_URL_HOST),
        parse_url($url, PHP_URL_PORT),
        parse_url('relative/path?query=yes'),
        parse_url('x://::abc/?'),
    ];
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = parse_url_exact_native();
}
var_dump($result);

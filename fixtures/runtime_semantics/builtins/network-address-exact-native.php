<?php
// runtime-semantics: category=builtins expect=pass

function run_native_network_addresses(): array
{
    $ipv4 = inet_pton('192.0.2.1');
    $ipv6 = inet_pton('2001:db8::1');

    return [
        ip2long('127.0.0.1'),
        ip2long('01.2.3.4'),
        long2ip(4294967295),
        bin2hex($ipv4),
        inet_ntop($ipv4),
        bin2hex($ipv6),
        inet_ntop($ipv6),
        inet_pton('not-an-address'),
        inet_ntop('short'),
    ];
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = run_native_network_addresses();
}

var_dump($result);

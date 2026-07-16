<?php

foreach ([
    '127.0.0.1',
    '0.0.0.0',
    '255.255.255.255',
    '01.2.3.4',
    '256.0.0.1',
    '192.168.0xa.5',
] as $address) {
    var_dump(ip2long($address));
}

foreach ([0, 1, -1, 4294967295, 4294967296] as $address) {
    var_dump(long2ip($address));
}

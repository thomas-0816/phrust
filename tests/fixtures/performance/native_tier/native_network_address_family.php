<?php

echo ip2long("127.0.0.1"), "\n";
echo long2ip(4294967295), "\n";
echo bin2hex(inet_pton("192.0.2.1")), "\n";
echo inet_ntop(hex2bin("20010db8000000000000000000000001")), "\n";
echo (ip2long("01.2.3.4") === false ? "1" : "0");
echo (inet_pton("not-an-address") === false ? "1" : "0"), "\n";
echo long2ip("2130706433"), "\n";

<?php

$input = 'native bytes~';
$base64 = base64_encode($input);
$hex = bin2hex($input);
$uuencoded = convert_uuencode($input);

echo $base64, "\n";
echo base64_decode($base64, true), "\n";
echo base64_decode('%%%', true) === false ? "false\n" : "bad\n";
echo $hex, "\n";
echo hex2bin($hex), "\n";
echo quoted_printable_decode('native=20bytes=7E'), "\n";
echo urlencode($input), "\n";
echo rawurlencode($input), "\n";
echo urldecode('native+bytes%7E'), "\n";
echo rawurldecode('native+bytes%7E'), "\n";
echo bin2hex($uuencoded), "\n";
echo convert_uudecode($uuencoded), "\n";

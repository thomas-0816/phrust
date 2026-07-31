<?php

$payload = "native zlib payload";
$gzip = gzencode($payload, 6, 0);
$zlib = gzcompress($payload, 6);
$raw = gzdeflate($payload);

echo gzdecode($gzip), "\n";
echo gzuncompress($zlib), "\n";
echo gzinflate($raw), "\n";
echo zlib_decode($gzip), ":", zlib_decode($zlib), ":", zlib_decode($raw), "\n";

$encodedRaw = zlib_encode($payload, -15, 6);
$encodedGzip = zlib_encode($payload, 31);
$encodedZlib = zlib_encode($payload, 15);
echo (zlib_decode($encodedRaw) === $payload ? "1" : "0");
echo (zlib_decode($encodedGzip) === $payload ? "1" : "0");
echo (zlib_decode($encodedZlib) === $payload ? "1" : "0"), "\n";

echo (gzdecode("invalid") === false ? "1" : "0");
echo (gzdecode($gzip, 4) === false ? "1" : "0"), "\n";
echo gzuncompress(gzcompress($payload, "6")), "\n";

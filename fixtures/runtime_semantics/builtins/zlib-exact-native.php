<?php
// runtime-semantics: category=builtins expect=pass

function run_native_zlib(string $payload): array
{
    $gzip = gzencode($payload, 6);
    $zlib = gzcompress($payload, 6);
    $raw = gzdeflate($payload, 6);
    $generic = zlib_encode($payload, ZLIB_ENCODING_GZIP, 6);

    return [
        gzdecode($gzip) === $payload,
        gzuncompress($zlib) === $payload,
        gzinflate($raw) === $payload,
        zlib_decode($gzip) === $payload,
        zlib_decode($zlib) === $payload,
        zlib_decode($raw) === $payload,
        zlib_decode($generic) === $payload,
    ];
}

$payload = str_repeat("native-zlib\0payload-", 128);
$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = run_native_zlib($payload);
}

$previousDisplayErrors = ini_set('display_errors', '0');
$previousLogErrors = ini_set('log_errors', '0');
$result[] = gzdecode('not compressed') === false;
ini_set('display_errors', $previousDisplayErrors);
ini_set('log_errors', $previousLogErrors);

var_dump($result);

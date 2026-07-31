<?php
// runtime-semantics: category=builtins expect=pass

function run_exact_hash(): array
{
    $input = 'Phrust native';
    $key = 'stable-key';
    $longKey = '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef-extra';
    $known = 'same';
    $knownAlias =& $known;
    $user = 'same';
    $userAlias =& $user;

    return [
        md5($input),
        bin2hex(md5($input, true)),
        sha1($input),
        bin2hex(sha1($input, true)),
        crc32($input),
        hash('sha256', $input),
        bin2hex(hash('sha256', $input, true)),
        hash('MD5', $input),
        hash('sha512/256', $input),
        hash('murmur3c', $input),
        bin2hex(hash('gost', $input, true)),
        hash('sha256', $input, false, []),
        hash_hmac('sha256', $input, $key),
        bin2hex(hash_hmac('sha256', $input, $key, true)),
        hash_hmac('md5', $input, $longKey),
        hash_hmac('sha3-256', $input, $longKey),
        hash_hmac('ripemd160', $input, $key),
        hash_hmac('whirlpool', $input, $key),
        hash_hmac('tiger192,3', $input, $key),
        hash_equals('same', 'same'),
        hash_equals('same', 'different'),
        hash_equals($knownAlias, $userAlias),
    ];
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = run_exact_hash();
}
var_dump($result);

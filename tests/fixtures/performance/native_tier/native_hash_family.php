<?php

$input = 'native';
$digest = hash('sha256', $input);

echo md5($input), "\n";
echo strlen(md5($input, true)), "\n";
echo sha1($input), "\n";
echo strlen(sha1($input, true)), "\n";
echo crc32($input), "\n";
echo $digest, "\n";
echo strlen(hash('sha256', $input, true)), "\n";
echo hash_hmac('sha256', $input, 'secret'), "\n";
echo strlen(hash_hmac('sha256', $input, 'secret', true)), "\n";
echo hash_equals($digest, hash('sha256', $input)) ? "yes\n" : "no\n";
echo hash_equals('a', 'b') ? "yes\n" : "no\n";

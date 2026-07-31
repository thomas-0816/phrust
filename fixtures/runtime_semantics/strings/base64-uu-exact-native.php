<?php
// runtime-semantics: category=strings expect=pass

function run_native_binary_codecs(string $input): array
{
    $base64 = base64_encode($input);
    $uuencoded = convert_uuencode($input);

    return [
        $base64,
        base64_decode($base64, true),
        base64_decode('a!Gk=', false),
        base64_decode('a!Gk=', true),
        base64_decode('aGk', true),
        base64_decode("a G\nk=\r", true),
        base64_decode('AB==', true),
        base64_decode('aG=k', false),
        $uuencoded,
        convert_uudecode($uuencoded),
        convert_uuencode(''),
        convert_uudecode("`\n"),
    ];
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = run_native_binary_codecs(
        "012345678901234567890123456789012345678901234\0\xff"
    );
}

var_dump($result);

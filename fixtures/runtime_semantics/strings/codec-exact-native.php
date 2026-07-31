<?php
// runtime-semantics: category=strings expect=pass

function run_native_codecs(string $input): array
{
    $packed = pack('n2VH4', 0x1234, 0xabcd, 0x89abcdef, '1a2b');

    return [
        bin2hex($input),
        hex2bin('00417fff'),
        quoted_printable_decode("alpha=20beta=\r\nnext=3D"),
        urlencode("a b+c/~\0"),
        rawurlencode("a b+c/~\0"),
        urldecode('a+b%2Bc%2F%7E%00%zz'),
        rawurldecode('a+b%2Bc%2F%7E%00%zz'),
        stripslashes("a\\'b\\0c\\\\d\\"),
        stripcslashes('a\n\x41\101\q\\'),
        quotemeta('.\+*?[^]$()plain'),
        bin2hex($packed),
        unpack('nfirst/nsecond/Vword/H4hex', "xx" . $packed, 2),
        unpack('nvalue/nvalue', $packed),
        unpack('n2', $packed),
    ];
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = run_native_codecs("native\0codec\xff");
}

var_dump($result);

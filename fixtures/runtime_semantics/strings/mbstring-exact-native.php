<?php
// runtime-semantics: category=strings expect=pass requires_ref_extension=mbstring php_ref_optional_reason=reference-build-lacks-mbstring

function run_exact_mbstring(): array
{
    mb_internal_encoding('UTF-8');
    mb_substitute_character(0xFFFD);

    $parsed = null;
    $parseResult = mb_parse_str('name=Gr%C3%BC%C3%9Fe&count=2', $parsed);
    $encodings = mb_list_encodings();
    $aliases = mb_encoding_aliases('UTF-8');
    $text = 'Grüße ÄÖÜ grüße';

    return [
        mb_detect_encoding($text, ['UTF-8', 'ISO-8859-1']),
        mb_check_encoding(),
        mb_check_encoding($text, 'UTF-8'),
        mb_convert_encoding($text, 'UTF-8'),
        mb_internal_encoding(),
        in_array('UTF-8', $encodings, true),
        in_array('utf8', $aliases, true),
        mb_substitute_character(),
        mb_strlen($text),
        mb_strtolower($text),
        mb_strtoupper($text),
        mb_stripos($text, 'äöü'),
        mb_strpos($text, 'ÄÖÜ'),
        mb_strripos($text, 'GRÜSSE'),
        mb_strrpos($text, 'grüße'),
        mb_substr_count($text, 'Grüße'),
        mb_substr($text, 0, 5),
        mb_strcut($text, 0, 7),
        mb_strwidth($text),
        mb_strimwidth($text, 0, 10, '…'),
        mb_convert_case($text, MB_CASE_TITLE),
        mb_ord('Ä'),
        mb_chr(196),
        $parseResult,
        $parsed,
    ];
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = run_exact_mbstring();
}
var_dump($result);

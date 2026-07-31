<?php
function native_string_structure_family(string $value, int $length): array {
    return [
        str_split($value),
        str_split($value, $length),
        str_split("", $length),
        version_compare("8.4.1", "8.4.0"),
        version_compare("8.4.0RC1", "8.4.0"),
        version_compare("8.4.1", "8.4.0", ">="),
        version_compare("8.4.0", "8.4.1", "lt"),
        strstr("alpha-BETA-tail", "BETA"),
        strstr("alpha-BETA-tail", "BETA", true),
        stristr("alpha-BETA-tail", "beta"),
        strrchr("abcabc", "b"),
        strrchr("abcabc", "b", true),
        strpbrk("abcdef", "xdy"),
        strpbrk("abcdef", "xyz"),
        bin2hex(strstr(hex2bin("ff00aa00bb"), hex2bin("00aa"))),
        substr_compare("alpha-BETA-tail", "beta", 6, 4, true),
        substr_compare("alpha-BETA-tail", "BET", 6, 3),
        substr_compare("abc", "abcd", 0),
    ];
}

var_dump(native_string_structure_family("native", 2));

<?php
function trim_charlist(string $value, string $characters): string {
    return trim($value, $characters);
}

function ltrim_charlist(string $value, string $characters): string {
    return ltrim($value, $characters);
}

function rtrim_charlist(string $value, string $characters): string {
    return rtrim($value, $characters);
}

echo trim_charlist('/alpha/', '/'), "\n";
echo ltrim_charlist('--beta-', '-'), "\n";
echo rtrim_charlist('gamma***', '*'), "\n";
echo trim_charlist('abcxyz', 'a..cz'), "\n";

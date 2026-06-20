#!/usr/bin/env php
<?php
declare(strict_types=1);

if (PHP_SAPI !== 'cli') {
    fwrite(STDERR, "dump-reference-tokens.php must run under the PHP CLI\n");
    exit(1);
}

$constants = get_defined_constants(true);
$tokens = [];

foreach ($constants as $group) {
    foreach ($group as $name => $value) {
        if (is_int($value) && str_starts_with($name, 'T_')) {
            $tokens[$name] = $value;
        }
    }
}

ksort($tokens, SORT_STRING);

$rows = [];
foreach ($tokens as $name => $value) {
    $rows[] = [
        'name' => $name,
        'value' => $value,
    ];
}

$output = [
    'php_version' => PHP_VERSION,
    'php_version_id' => PHP_VERSION_ID,
    'generated_at' => gmdate(DATE_ATOM),
    'tokens' => $rows,
];

echo json_encode(
    $output,
    JSON_PRETTY_PRINT
        | JSON_UNESCAPED_SLASHES
        | JSON_UNESCAPED_UNICODE
        | JSON_INVALID_UTF8_SUBSTITUTE
) . "\n";

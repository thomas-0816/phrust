#!/usr/bin/env php
<?php
declare(strict_types=1);

function usage(): void
{
    $script = basename(__FILE__);
    fwrite(STDOUT, <<<TEXT
Usage:
  php scripts/$script --file path/to/file.php [--token-parse]

Options:
  --file PATH      PHP source file to tokenize.
  --token-parse   Pass TOKEN_PARSE to token_get_all().
  --help          Show this help.

Output:
  UTF-8 JSON. Invalid UTF-8 token bytes are substituted during JSON encoding.

TEXT);
}

function fail(string $message, int $code = 1): never
{
    fwrite(STDERR, $message . "\n");
    exit($code);
}

if (PHP_SAPI !== 'cli') {
    fail('tokenize-reference.php must run under the PHP CLI');
}

$file = null;
$tokenParse = false;

for ($index = 1; $index < $argc; $index++) {
    $arg = $argv[$index];
    if ($arg === '--help' || $arg === '-h') {
        usage();
        exit(0);
    }
    if ($arg === '--token-parse') {
        $tokenParse = true;
        continue;
    }
    if ($arg === '--file') {
        if (!isset($argv[$index + 1])) {
            fail('--file requires a path');
        }
        $file = $argv[++$index];
        continue;
    }
    fail("Unknown argument: {$arg}");
}

if ($file === null) {
    usage();
    fail('--file is required');
}

if (!is_file($file) || !is_readable($file)) {
    fail("Cannot read source file: {$file}");
}

$source = file_get_contents($file);
if ($source === false) {
    fail("Failed to read source file: {$file}");
}

$flags = $tokenParse ? TOKEN_PARSE : 0;

try {
    $rawTokens = token_get_all($source, $flags);
} catch (Throwable $throwable) {
    fail($throwable::class . ': ' . $throwable->getMessage());
}

$tokens = [];
$line = 1;

foreach ($rawTokens as $index => $token) {
    if (is_array($token)) {
        [$id, $text, $tokenLine] = $token;
        $kind = token_name($id);
        $line = (int) $tokenLine;
    } else {
        $text = $token;
        $kind = $token;
    }

    $tokens[] = [
        'index' => $index,
        'kind' => $kind,
        'text' => $text,
        'line' => $line,
    ];

    $line += substr_count($text, "\n");
}

$output = [
    'php_version' => PHP_VERSION,
    'php_version_id' => PHP_VERSION_ID,
    'file' => $file,
    'token_parse' => $tokenParse,
    'tokens' => $tokens,
];

$json = json_encode(
    $output,
    JSON_PRETTY_PRINT
        | JSON_UNESCAPED_SLASHES
        | JSON_UNESCAPED_UNICODE
        | JSON_INVALID_UTF8_SUBSTITUTE
);

if ($json === false) {
    fail('Failed to encode token JSON: ' . json_last_error_msg());
}

echo $json . "\n";

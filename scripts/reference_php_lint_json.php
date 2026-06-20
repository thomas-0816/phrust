#!/usr/bin/env php
<?php
declare(strict_types=1);

function usage(): never {
    fwrite(STDERR, "Usage: reference_php_lint_json.php --file path/to/file.php\n");
    exit(2);
}

$file = null;
for ($i = 1; $i < $argc; $i++) {
    if ($argv[$i] === '--file') {
        $i++;
        if ($i >= $argc) {
            usage();
        }
        $file = $argv[$i];
    } elseif ($argv[$i] === '--help' || $argv[$i] === '-h') {
        usage();
    } else {
        usage();
    }
}

if ($file === null) {
    usage();
}

$descriptors = [
    1 => ['pipe', 'w'],
    2 => ['pipe', 'w'],
];

$process = proc_open([PHP_BINARY, '-l', $file], $descriptors, $pipes);
if (!is_resource($process)) {
    fwrite(STDERR, "failed to execute php -l\n");
    exit(1);
}

$stdout = stream_get_contents($pipes[1]);
$stderr = stream_get_contents($pipes[2]);
fclose($pipes[1]);
fclose($pipes[2]);
$exitCode = proc_close($process);

$result = [
    'file' => $file,
    'ok' => $exitCode === 0,
    'exit_code' => $exitCode,
    'stdout' => $stdout,
    'stderr' => $stderr,
    'php_version' => PHP_VERSION,
];

echo json_encode($result, JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE) . "\n";

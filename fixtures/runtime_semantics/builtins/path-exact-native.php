<?php
// runtime-semantics: category=builtins expect=pass

function run_exact_path_builtins(): array
{
    $path = __FILE__;
    $source = fopen('php://memory', 'w+');
    $written = fwrite($source, "alpha\nbeta\n", 11);
    $flushed = fflush($source);
    $rewound = rewind($source);
    $line = fgets($source);
    $position = ftell($source);
    $character = fgetc($source);
    $seeked = fseek($source, -4, SEEK_END);
    $tail = fread($source, 4);

    $destination = fopen('php://memory', 'w+');
    rewind($source);
    $copied = stream_copy_to_stream($source, $destination, 4, 6);
    rewind($destination);
    $copy = stream_get_contents($destination);
    $truncated = ftruncate($destination, 2);
    rewind($destination);
    $short = stream_get_contents($destination, -1, 0);

    fseek($source, 0, SEEK_END);
    $pastEnd = fgetc($source);
    $eof = feof($source);
    $sourceClosed = fclose($source);
    $destinationClosed = fclose($destination);
    $stat = stat($path);
    $lstat = lstat($path);
    $lines = file($path, FILE_IGNORE_NEW_LINES);
    $glob = glob(__DIR__ . '/path-exact-native.php');

    return [
        basename('/srv/www/index.php'),
        basename('/srv/www/index.php', '.php'),
        dirname('/srv/www/wp-content/plugins', 2),
        basename(realpath($path)),
        realpath(__DIR__ . '/definitely-missing-native-path'),
        file_exists($path),
        is_file($path),
        is_dir(dirname($path)),
        is_readable($path),
        is_writable($path),
        is_link($path),
        fileperms($path) > 0,
        fileowner($path) >= 0,
        filegroup($path) >= 0,
        filetype($path),
        disk_free_space(dirname($path)) > 0,
        disk_total_space(dirname($path)) > 0,
        pathinfo($path),
        pathinfo($path, PATHINFO_FILENAME),
        [$stat['mode'] > 0, $stat['size'] > 0, $stat['mtime'] > 0],
        [$lstat['mode'] > 0, $lstat['size'] > 0, $lstat['mtime'] > 0],
        [count($lines) > 0, $lines[0]],
        [count($glob), basename($glob[0])],
        filesize($path) > 0,
        filemtime($path) > 0,
        file_get_contents($path, false, null, 0, 5),
        $written,
        $flushed,
        $rewound,
        $line,
        $position,
        $character,
        $seeked,
        $tail,
        $copied,
        $copy,
        $truncated,
        $short,
        $pastEnd,
        $eof,
        $sourceClosed,
        $destinationClosed,
    ];
}

function run_exact_path_mutations(string $root, int $iteration): array
{
    $directory = $root . '/dir-' . $iteration;
    $source = $directory . '/source.txt';
    $renamed = $directory . '/renamed.txt';
    $touched = $directory . '/touched.txt';

    $made = mkdir($directory);
    $written = file_put_contents($source, 'alpha');
    $appended = file_put_contents($source, '-beta', FILE_APPEND);
    $moved = rename($source, $renamed);
    $contents = file_get_contents($renamed);
    $created = touch($touched);
    $removedFile = unlink($renamed);
    $removedTouched = unlink($touched);
    $removedDirectory = rmdir($directory);

    return [
        $made,
        $written,
        $appended,
        $moved,
        $contents,
        $created,
        $removedFile,
        $removedTouched,
        $removedDirectory,
    ];
}

$mutationRoot = sys_get_temp_dir() . '/' . uniqid('phrust-native-path-', true);
mkdir($mutationRoot);
$result = null;
$mutationResult = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = run_exact_path_builtins();
    $mutationResult = run_exact_path_mutations($mutationRoot, $iteration);
}
rmdir($mutationRoot);
var_dump($result);
var_dump($mutationResult);

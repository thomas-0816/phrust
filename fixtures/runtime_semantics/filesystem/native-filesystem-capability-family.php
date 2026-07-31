<?php

$base = __DIR__ . '/_native-filesystem-capability';
$source = $base . '.txt';
$link = $base . '.link';
@unlink($link);
@unlink($source);

file_put_contents($source, 'native-filesystem');
$chmod = chmod($source, 0644);
$symlink = symlink($source, $link);

ob_start();
$readLength = readfile($link);
$readBytes = ob_get_clean();

$uploaded = is_uploaded_file($source);
$temporary = tempnam(__DIR__, 'nfc-');
$temporaryCreated = is_string($temporary) && file_exists($temporary);

$stream = tmpfile();
$streamType = get_resource_type($stream);
fwrite($stream, 'temporary-stream');
rewind($stream);
$streamBytes = stream_get_contents($stream);
fclose($stream);

if (is_string($temporary)) {
    unlink($temporary);
}
unlink($link);
unlink($source);

echo json_encode([
    'chmod' => $chmod,
    'symlink' => $symlink,
    'readLength' => $readLength,
    'readBytes' => $readBytes,
    'uploaded' => $uploaded,
    'temporaryCreated' => $temporaryCreated,
    'streamType' => $streamType,
    'streamBytes' => $streamBytes,
]), "\n";

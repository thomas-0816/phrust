<?php
// native-filesystem-family

$missing = __DIR__ . '/native-filesystem-query-read-missing.txt';
$contents = file_get_contents(__FILE__);

echo 'is_file=', is_file(__FILE__) ? '1' : '0', "\n";
echo 'is_dir=', is_dir(__DIR__) ? '1' : '0', "\n";
echo 'is_readable=', is_readable(__FILE__) ? '1' : '0', "\n";
echo 'is_writable=', is_writable(__FILE__) ? '1' : '0', "\n";
echo 'filesize=', filesize(__FILE__) > 0 ? '1' : '0', "\n";
echo 'filemtime=', filemtime(__FILE__) > 0 ? '1' : '0', "\n";
echo 'contents=', strpos($contents, 'native-filesystem-family') !== false ? '1' : '0', "\n";
echo 'slice=', file_get_contents(__FILE__, false, null, 0, 5) === '<?php' ? '1' : '0', "\n";
echo 'wrapper=', file_get_contents('php://memory') === '' ? '1' : '0', "\n";
echo 'missing_file=', @is_file($missing) ? '1' : '0', "\n";
echo 'missing_dir=', @is_dir($missing) ? '1' : '0', "\n";
echo 'missing_size=', @filesize($missing) === false ? '1' : '0', "\n";
echo 'missing_mtime=', @filemtime($missing) === false ? '1' : '0', "\n";
echo 'missing_contents=', @file_get_contents($missing) === false ? '1' : '0', "\n";

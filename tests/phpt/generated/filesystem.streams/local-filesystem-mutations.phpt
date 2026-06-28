--TEST--
filesystem.streams: local filesystem mutations
--DESCRIPTION--
Generated local filesystem baseline covering mkdir, is_dir, file_put_contents,
is_file, filesize, filemtime, readfile, rename, file_get_contents, unlink, and
rmdir under deterministic local root constraints.
--FILE--
<?php
$dir = __DIR__ . "/local-filesystem-mutations-dir";
$path = $dir . "/source.txt";
$renamed = $dir . "/renamed.txt";
@unlink($path);
@unlink($renamed);
@rmdir($dir);
var_dump(mkdir($dir));
var_dump(is_dir($dir));
var_dump(file_put_contents($path, "hello"));
var_dump(is_file($path));
var_dump(filesize($path));
var_dump(filemtime($path) > 0);
echo "read:";
$bytes = readfile($path);
echo "|";
var_dump($bytes);
var_dump(rename($path, $renamed));
var_dump(file_exists($path));
var_dump(file_get_contents($renamed));
var_dump(unlink($renamed));
var_dump(rmdir($dir));
?>
--CLEAN--
<?php
$dir = __DIR__ . "/local-filesystem-mutations-dir";
@unlink($dir . "/source.txt");
@unlink($dir . "/renamed.txt");
@rmdir($dir);
?>
--EXPECT--
bool(true)
bool(true)
int(5)
bool(true)
int(5)
bool(true)
read:hello|int(5)
bool(true)
bool(false)
string(5) "hello"
bool(true)
bool(true)

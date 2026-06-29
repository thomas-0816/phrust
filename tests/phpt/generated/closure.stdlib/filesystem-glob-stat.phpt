--TEST--
closure.stdlib: local filesystem stat and glob helpers
--DESCRIPTION--
Generated closure stdlib coverage for local file writes, stat helpers,
copy/rename/unlink, realpath, and simple glob patterns.
--FILE--
<?php
$dir = __DIR__ . "/closure-stdlib-files";
$nested = $dir . "/nested";
$path = $nested . "/alpha.txt";
$copy = $nested . "/copy.txt";
$renamed = $nested . "/renamed.txt";
@unlink($path);
@unlink($copy);
@unlink($renamed);
@rmdir($nested);
@rmdir($dir);

var_dump(mkdir($nested, 0777, true));
var_dump(file_put_contents($path, "alpha"));
var_dump(is_file($path));
var_dump(filesize($path));
var_dump(filetype($path));
$stat = stat($path);
var_dump($stat["size"]);
var_dump(copy($path, $copy));
var_dump(rename($copy, $renamed));
var_dump(file_exists($copy));
var_dump(file_get_contents($renamed));
$matches = glob($nested . "/*.txt");
sort($matches);
var_dump(count($matches));
var_dump(is_string(realpath($path)));
var_dump(unlink($path));
var_dump(unlink($renamed));
var_dump(rmdir($nested));
var_dump(rmdir($dir));
?>
--CLEAN--
<?php
$dir = __DIR__ . "/closure-stdlib-files";
@unlink($dir . "/nested/alpha.txt");
@unlink($dir . "/nested/copy.txt");
@unlink($dir . "/nested/renamed.txt");
@rmdir($dir . "/nested");
@rmdir($dir);
?>
--EXPECT--
bool(true)
int(5)
bool(true)
int(5)
string(4) "file"
int(5)
bool(true)
bool(true)
bool(false)
string(5) "alpha"
int(2)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)

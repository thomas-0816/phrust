--TEST--
zip: unchange operations and store/default compression setters
--DESCRIPTION--
Generated ZipArchive coverage for reverting pending mutations and supported stored-entry compression metadata.
--SKIPIF--
<?php
if (!extension_loaded("zip")) die("skip zip extension not available");
?>
--FILE--
<?php
$dir = __DIR__ . "/zip-unchange-compression-store";
$zipPath = $dir . "/fixture.zip";
@unlink($zipPath);
@rmdir($dir);
mkdir($dir);

$zip = new ZipArchive();
var_dump($zip->open($zipPath, ZipArchive::CREATE | ZipArchive::OVERWRITE));
var_dump($zip->addFromString("first.txt", "one"));
var_dump($zip->addFromString("second.txt", "two"));
var_dump($zip->setArchiveComment("original archive"));
var_dump($zip->setCommentName("first.txt", "original file"));
var_dump($zip->close());

$zip = new ZipArchive();
var_dump($zip->open($zipPath));
var_dump($zip->renameIndex(0, "renamed.txt"));
var_dump($zip->setCommentName("renamed.txt", "changed file"));
var_dump($zip->getNameIndex(0));
var_dump($zip->getCommentIndex(0));
var_dump($zip->unchangeName("renamed.txt"));
var_dump($zip->getNameIndex(0));
var_dump($zip->getCommentName("first.txt"));

var_dump($zip->setArchiveComment("changed archive"));
var_dump($zip->getArchiveComment());
var_dump($zip->unchangeArchive());
var_dump($zip->getArchiveComment());

var_dump($zip->addFromString("new.txt", "new"));
var_dump($zip->count());
var_dump($zip->unchangeIndex(2));
var_dump($zip->count());

var_dump($zip->renameIndex(1, "renamed-second.txt"));
var_dump($zip->setCommentIndex(1, "changed second"));
var_dump($zip->unchangeAll());
var_dump($zip->getNameIndex(0));
var_dump($zip->getNameIndex(1));
var_dump($zip->getCommentIndex(1));

var_dump($zip->setCompressionName("first.txt", ZipArchive::CM_STORE));
var_dump($zip->setCompressionIndex(0, ZipArchive::CM_DEFAULT));
var_dump($zip->close());

$zip = new ZipArchive();
var_dump($zip->open($zipPath, ZipArchive::RDONLY));
var_dump($zip->getFromName("first.txt"));
var_dump($zip->getFromName("second.txt"));
var_dump($zip->getArchiveComment());
var_dump($zip->getCommentName("first.txt"));
var_dump($zip->close());
?>
--CLEAN--
<?php
$dir = __DIR__ . "/zip-unchange-compression-store";
@unlink($dir . "/fixture.zip");
@rmdir($dir);
?>
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
string(11) "renamed.txt"
string(12) "changed file"
bool(true)
string(9) "first.txt"
string(13) "original file"
bool(true)
string(15) "changed archive"
bool(true)
string(16) "original archive"
bool(true)
int(3)
bool(true)
int(2)
bool(true)
bool(true)
bool(true)
string(9) "first.txt"
string(10) "second.txt"
string(0) ""
bool(true)
bool(true)
bool(true)
bool(true)
string(3) "one"
string(3) "two"
string(16) "original archive"
string(13) "original file"
bool(true)

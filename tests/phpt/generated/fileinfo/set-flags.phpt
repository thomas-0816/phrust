--TEST--
fileinfo: procedural finfo_set_flags updates stored flags
--DESCRIPTION--
Generated procedural Fileinfo coverage for stored resource flags without the
upstream OO finfo class dependency.
--SKIPIF--
<?php
if (!extension_loaded("fileinfo")) die("skip fileinfo extension not available");
?>
--FILE--
<?php
$finfo = finfo_open(FILEINFO_NONE);
var_dump(finfo_set_flags($finfo, FILEINFO_MIME_TYPE));
var_dump(finfo_buffer($finfo, "Regular string here"));
var_dump(finfo_set_flags($finfo, FILEINFO_MIME_ENCODING));
var_dump(finfo_buffer($finfo, "Regular string here"));
var_dump(finfo_set_flags($finfo, FILEINFO_MIME));
var_dump(finfo_buffer($finfo, "Regular string here"));
?>
--EXPECT--
bool(true)
string(10) "text/plain"
bool(true)
string(8) "us-ascii"
bool(true)
string(28) "text/plain; charset=us-ascii"

--TEST--
filesystem.streams: stream seek contents and eof
--DESCRIPTION--
Generated stream baseline covering php://memory resource identity, fseek,
ftell, stream_get_contents, feof, rewind, fread, and fclose.
--FILE--
<?php
$stream = fopen("php://memory", "w+");
var_dump(is_resource($stream));
var_dump(fwrite($stream, "abcdef"));
var_dump(ftell($stream));
var_dump(fseek($stream, 2));
var_dump(ftell($stream));
var_dump(stream_get_contents($stream, 2));
var_dump(feof($stream));
var_dump(stream_get_contents($stream));
var_dump(feof($stream));
rewind($stream);
var_dump(fread($stream, 3));
var_dump(fclose($stream));
?>
--EXPECT--
bool(true)
int(6)
int(6)
int(0)
int(2)
string(2) "cd"
bool(false)
string(2) "ef"
bool(true)
string(3) "abc"
bool(true)

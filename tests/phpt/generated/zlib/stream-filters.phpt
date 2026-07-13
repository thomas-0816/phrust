--TEST--
zlib: stream filter append, prepend, and removal
--DESCRIPTION--
Generated stream filter coverage for zlib.deflate and zlib.inflate.
--SKIPIF--
<?php
if (!extension_loaded("zlib")) die("skip zlib extension not available");
?>
--FILE--
<?php
$payload = "alpha\nbeta\nomega";

$stream = fopen("php://temp", "w+");
$filter = stream_filter_append($stream, "zlib.deflate", STREAM_FILTER_WRITE);
var_dump(is_resource($filter));
var_dump(get_resource_type($filter));
var_dump(fwrite($stream, $payload));
var_dump(fflush($stream));
var_dump(stream_filter_remove($filter));
rewind($stream);
$compressed = stream_get_contents($stream);
var_dump(strlen($compressed) > 0);
var_dump($compressed === $payload);

$stream = fopen("php://temp", "w+");
fwrite($stream, $compressed);
rewind($stream);
$filter = stream_filter_prepend($stream, "zlib.inflate", STREAM_FILTER_READ);
var_dump(is_resource($filter));
var_dump(stream_get_contents($stream));
var_dump(stream_filter_remove($filter));

$stream = fopen("php://temp", "w+");
var_dump(stream_filter_append($stream, "zlib.missing", STREAM_FILTER_WRITE));
?>
--EXPECTF--
bool(true)
string(13) "stream filter"
int(16)
bool(true)
bool(true)
bool(true)
bool(false)
bool(true)
string(16) "alpha
beta
omega"
bool(true)

Warning: stream_filter_append(): Unable to create or locate filter `zlib.missing` in %s on line %d
bool(false)

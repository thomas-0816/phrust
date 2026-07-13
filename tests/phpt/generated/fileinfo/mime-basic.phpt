--TEST--
fileinfo: common upload MIME detection MVP
--DESCRIPTION--
Generated media/archive MIME coverage for deterministic file and buffer
sniffing without depending on a host libmagic database.
--SKIPIF--
<?php
if (!extension_loaded("fileinfo")) die("skip fileinfo extension not available");
?>
--FILE--
<?php
$dir = __DIR__ . "/fileinfo-mime-basic";
$png = $dir . "/tiny.png";
$pdf = $dir . "/doc.pdf";
$txt = $dir . "/readme.txt";
@unlink($png);
@unlink($pdf);
@unlink($txt);
@rmdir($dir);
mkdir($dir);
file_put_contents($png, "\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR\x00\x00\x00\x02\x00\x00\x00\x03");
file_put_contents($pdf, "%PDF-1.7\n");
file_put_contents($txt, "plain text\n");
$zip = "PK\x05\x06" . str_repeat("\0", 18);
$finfo = finfo_open(FILEINFO_MIME_TYPE);
var_dump($finfo instanceof finfo);
var_dump(finfo_file($finfo, $png));
var_dump(finfo_file($finfo, $txt));
var_dump(finfo_buffer($finfo, $zip));
var_dump(finfo_buffer($finfo, "{\"ok\":true}"));
var_dump(finfo_buffer($finfo, "<?xml version=\"1.0\"?>"));
var_dump(mime_content_type($pdf));
var_dump(finfo_close($finfo));
?>
--CLEAN--
<?php
$dir = __DIR__ . "/fileinfo-mime-basic";
@unlink($dir . "/tiny.png");
@unlink($dir . "/doc.pdf");
@unlink($dir . "/readme.txt");
@rmdir($dir);
?>
--EXPECT--
bool(true)
string(9) "image/png"
string(10) "text/plain"
string(15) "application/zip"
string(16) "application/json"
string(8) "text/xml"
string(15) "application/pdf"
bool(true)

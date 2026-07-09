--TEST--
iconv_mime_decode_headers selected basics
--SKIPIF--
<?php if (!extension_loaded("iconv")) die("skip iconv extension not loaded"); ?>
--FILE--
<?php
$headers = "Subject: =?utf-8?B?UHLDvGZ1bmc=?=\r\n"
    . "From: Alice <a@example.com>\r\n"
    . "X-Test: =?UTF-8?Q?hello_world?=\r\n";
$decoded = iconv_mime_decode_headers($headers, 0, "UTF-8");
var_dump(bin2hex($decoded["Subject"]));
var_dump($decoded["From"]);
var_dump($decoded["X-Test"]);

$duplicates = iconv_mime_decode_headers("Subject: one\r\nSubject: two\r\n", 0, "UTF-8");
var_dump($duplicates["Subject"]);
?>
--EXPECT--
string(16) "5072c3bc66756e67"
string(21) "Alice <a@example.com>"
string(11) "hello world"
array(2) {
  [0]=>
  string(3) "one"
  [1]=>
  string(3) "two"
}

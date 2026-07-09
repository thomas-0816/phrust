--TEST--
standard.strings: pack and unpack hex nibble formats
--FILE--
<?php
echo bin2hex(pack("H*", "0061f")), "\n";
echo bin2hex(pack("h*", "0061f")), "\n";
echo bin2hex(pack("H3", "0061f")), "\n";
var_dump(unpack("H3a/H2b", "\x01\x23\x45"));
var_dump(unpack("h*hex", "\x00\x61\xff"));
?>
--EXPECT--
0061f0
00160f
0060
array(2) {
  ["a"]=>
  string(3) "012"
  ["b"]=>
  string(2) "45"
}
array(1) {
  ["hex"]=>
  string(6) "0016ff"
}

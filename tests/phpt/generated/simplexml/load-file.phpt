--TEST--
simplexml: load file MVP
--DESCRIPTION--
Generated SimpleXML coverage for local file loading through the bounded XML
tree.
--EXTENSIONS--
simplexml
--FILE--
<?php
$path = __DIR__ . "/simplexml-load-file.xml";
var_dump(file_put_contents($path, '<root id="9"><child>file</child></root>'));
$xml = simplexml_load_file($path);
echo $xml->child, "\n";
echo $xml->attributes()->id, "\n";
var_dump(unlink($path));
?>
--EXPECT--
int(39)
file
9
bool(true)

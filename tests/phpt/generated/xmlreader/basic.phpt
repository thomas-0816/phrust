--TEST--
xmlreader: XML/read/node properties/getAttribute/close MVP
--DESCRIPTION--
Generated XMLReader coverage for forward-only traversal over the bounded XML tree.
--EXTENSIONS--
xmlreader
--FILE--
<?php
$reader = new XMLReader();
var_dump($reader->XML('<root id="7"><child>A</child></root>'));
while ($reader->read()) {
    echo $reader->nodeType, "|", $reader->name, "|", $reader->value, "|", var_export($reader->getAttribute("id"), true), "\n";
}
var_dump($reader->close());
?>
--EXPECT--
bool(true)
1|root||'7'
1|child||NULL
3||A|NULL
15|child||NULL
15|root||NULL
bool(true)

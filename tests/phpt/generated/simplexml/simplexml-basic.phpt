--TEST--
simplexml: load string, text, attributes, iteration, and asXML
--DESCRIPTION--
Generated SimpleXML MVP coverage for WordPress-style XML probing.
--EXTENSIONS--
simplexml
--FILE--
<?php
$xml = simplexml_load_string('<root id="7"><child>A &amp; B</child></root>');
echo $xml->asXML(), "\n";
echo $xml->child, "\n";
$attrs = $xml->attributes();
echo $attrs->id, "\n";
foreach ($xml as $name => $value) {
    echo $name, "=", $value, "\n";
}
?>
--EXPECT--
<root id="7"><child>A &amp; B</child></root>
A & B
7
child=A & B

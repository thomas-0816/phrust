--TEST--
dom: DOMDocument loadXML/saveXML/documentElement MVP
--DESCRIPTION--
Generated DOM coverage for strict XML load, serialization, and root element properties.
--EXTENSIONS--
dom
--FILE--
<?php
$document = new DOMDocument();
var_dump($document->loadXML('<root id="7"><child>A &amp; B</child></root>'));
echo $document->saveXML(), "\n";
echo $document->documentElement->tagName, "|", $document->documentElement->textContent, "\n";
?>
--EXPECT--
bool(true)
<root id="7"><child>A &amp; B</child></root>
root|A & B

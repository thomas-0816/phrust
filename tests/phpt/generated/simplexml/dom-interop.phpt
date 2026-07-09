--TEST--
simplexml: bounded DOM import/export MVP
--DESCRIPTION--
Generated SimpleXML coverage for bounded simplexml_import_dom() and
dom_import_simplexml() over the shared XML tree representation.
--EXTENSIONS--
simplexml
dom
--FILE--
<?php
$document = new DOMDocument();
$document->loadXML('<root id="7"><child>A &amp; B</child></root>');
$sx = simplexml_import_dom($document->documentElement);
var_dump($sx instanceof SimpleXMLElement);
echo $sx->getName(), "|", $sx->attributes()->id, "|", $sx->child, "\n";
echo $sx->asXML(), "\n";

$xml = simplexml_load_string('<outer><item code="x">Value</item></outer>');
$dom = dom_import_simplexml($xml->item);
var_dump($dom instanceof DOMElement);
echo $dom->tagName, "|", $dom->getAttribute('code'), "|", $dom->textContent, "\n";
echo $dom->getElementsByTagName('item')->length, "\n";
?>
--EXPECT--
bool(true)
root|7|A & B
<root id="7"><child>A &amp; B</child></root>
bool(true)
item|x|Value
1

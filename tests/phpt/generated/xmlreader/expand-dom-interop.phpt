--TEST--
xmlreader: expand DOM interop MVP
--DESCRIPTION--
Generated XMLReader coverage for bounded expand() support returning DOMElement nodes.
--EXTENSIONS--
xmlreader
dom
--FILE--
<?php
$reader = new XMLReader();
var_dump(method_exists("XMLReader", "expand"));
var_dump($reader->XML('<root><item code="x">Value</item><item code="y">Next</item></root>'));
var_dump($reader->read());
var_dump($reader->read());
$node = $reader->expand();
var_dump($node instanceof DOMElement);
echo $node->tagName, "|", $node->getAttribute('code'), "|", $node->textContent, "\n";
var_dump($reader->moveToFirstAttribute());
var_dump($reader->expand());
var_dump($reader->moveToElement());
var_dump($reader->next('item'));
$next = $reader->expand();
var_dump($next instanceof DOMElement);
echo $next->tagName, "|", $next->getAttribute('code'), "|", $next->textContent, "\n";
?>
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
item|x|Value
bool(true)
bool(false)
bool(true)
bool(true)
bool(true)
item|y|Next

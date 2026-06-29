--TEST--
dom: DOMDocument node-list and mutation MVP
--DESCRIPTION--
Generated DOM coverage for createElement, appendChild, attributes, node values, and DOMNodeList iteration.
--EXTENSIONS--
dom
--FILE--
<?php
$document = new DOMDocument();
$root = $document->createElement('root');
$root->setAttribute('id', '7');
$child = $document->createElement('item', 'Alpha');
$root->appendChild($child);
$document->appendChild($root);
echo $document->saveXML(), "\n";
echo $document->documentElement->nodeName, "|", $document->documentElement->nodeValue, "\n";
echo $document->documentElement->getAttribute('id'), "\n";
$items = $document->getElementsByTagName('item');
echo count($items), "|", $items->length, "|", $items->item(0)->nodeValue, "\n";
foreach ($items as $index => $node) {
    echo $index, ":", $node->tagName, "=", $node->nodeValue, "\n";
}
?>
--EXPECT--
<root id="7"><item>Alpha</item></root>
root|Alpha
7
1|1|Alpha
0:item=Alpha

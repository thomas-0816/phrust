--TEST--
dom: libxml-backed loadXML preserves declarations, namespaces, comments, and CDATA
--DESCRIPTION--
Generated DOM coverage for libxml-backed loadXML projection and saveXML roundtrip.
--EXTENSIONS--
dom
--FILE--
<?php
$xml = '<?xml version="1.0"?><root xmlns:h="urn:h" id="7"><!--note--><h:item a="b"><![CDATA[A & B]]></h:item></root>';
$document = new DOMDocument();
var_dump($document->loadXML($xml));
echo $document->saveXML(), "\n";
echo $document->documentElement->tagName, "|", $document->documentElement->getAttribute("id"), "|", $document->documentElement->getAttribute("xmlns:h"), "|", $document->documentElement->textContent, "\n";
?>
--EXPECT--
bool(true)
<root xmlns:h="urn:h" id="7"><!--note--><h:item a="b"><![CDATA[A & B]]></h:item></root>
root|7|urn:h|A & B

--TEST--
simplexml: libxml-backed namespaces, comments, and CDATA traversal
--DESCRIPTION--
Generated SimpleXML coverage for libxml-backed parsing of declarations, namespace declarations, comments, and CDATA.
--EXTENSIONS--
simplexml
--FILE--
<?php
$xml = '<?xml version="1.0"?><root xmlns:h="urn:h" id="7"><!--note--><h:item a="b"><![CDATA[A & B]]></h:item></root>';
$sx = simplexml_load_string($xml);
echo $sx->asXML(), "\n";
echo $sx->getName(), "|", $sx["id"], "\n";
foreach ($sx->children() as $name => $value) {
    echo $name, "=", $value, "|", $value->getName(), "\n";
}
?>
--EXPECT--
<root xmlns:h="urn:h" id="7"><!--note--><h:item a="b"><![CDATA[A & B]]></h:item></root>
root|7
h:item=A & B|h:item

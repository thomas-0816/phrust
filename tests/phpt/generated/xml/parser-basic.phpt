--TEST--
xml: bounded strict parser accepts simple XML and rejects unresolved entities
--DESCRIPTION--
Generated XML parser coverage for the local entity-safe parser MVP.
--EXTENSIONS--
xml
--FILE--
<?php
$parser = xml_parser_create();
var_dump(xml_parse($parser, '<root id="7"><child>A &amp; B</child></root>', true));
var_dump(xml_parse($parser, '<root>&bogus;</root>', true));
var_dump(xml_parse($parser, '<root><child></root>', true));
?>
--EXPECT--
int(1)
int(0)
int(0)

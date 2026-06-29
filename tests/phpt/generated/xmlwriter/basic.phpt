--TEST--
xmlwriter: memory writer element, attribute, text, and output MVP
--DESCRIPTION--
Generated XMLWriter coverage for deterministic in-memory XML serialization.
--EXTENSIONS--
xmlwriter
--FILE--
<?php
$writer = new XMLWriter();
var_dump($writer->openMemory());
var_dump($writer->startDocument());
var_dump($writer->startElement("root"));
var_dump($writer->writeAttribute("id", "7"));
var_dump($writer->text("A & B"));
var_dump($writer->endElement());
var_dump($writer->endDocument());
echo $writer->outputMemory(), "\n";
?>
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
<?xml version="1.0"?><root id="7">A &amp; B</root>

--TEST--
xmlwriter: comments and CDATA writers
--DESCRIPTION--
Generated XMLWriter coverage for bounded comment and CDATA object/procedural writers.
--EXTENSIONS--
xmlwriter
--FILE--
<?php
var_dump(function_exists("xmlwriter_write_comment"));
var_dump(function_exists("xmlwriter_write_cdata"));
var_dump(method_exists("XMLWriter", "writeComment"));
var_dump(method_exists("XMLWriter", "writeCdata"));

$writer = XMLWriter::toMemory();
var_dump($writer->startDocument());
var_dump($writer->startElement("root"));
var_dump($writer->writeComment("note"));
var_dump($writer->writeCdata("A < B & C"));
var_dump($writer->endDocument());
echo $writer->outputMemory(), "\n";

$writer = xmlwriter_open_memory();
var_dump(xmlwriter_start_document($writer));
var_dump(xmlwriter_start_element($writer, "root"));
var_dump(xmlwriter_write_comment($writer, "procedural"));
var_dump(xmlwriter_write_cdata($writer, "<raw> & text"));
var_dump(xmlwriter_end_document($writer));
echo xmlwriter_output_memory($writer), "\n";
?>
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
<?xml version="1.0"?><root><!--note--><![CDATA[A < B & C]]></root>
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
<?xml version="1.0"?><root><!--procedural--><![CDATA[<raw> & text]]></root>

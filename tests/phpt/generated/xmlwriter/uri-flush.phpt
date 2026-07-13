--TEST--
xmlwriter: URI writer and flush behavior
--DESCRIPTION--
Generated XMLWriter coverage for object, static, and procedural URI writers.
--EXTENSIONS--
xmlwriter
--FILE--
<?php
var_dump(method_exists("XMLWriter", "openUri"));
var_dump(method_exists("XMLWriter", "toUri"));
var_dump(method_exists("XMLWriter", "flush"));
var_dump(function_exists("xmlwriter_open_uri"));
var_dump(function_exists("xmlwriter_flush"));

$path = tempnam(sys_get_temp_dir(), "phrust_xmlwriter_uri_");
$writer = new XMLWriter();
var_dump($writer->openUri($path));
var_dump($writer->writeElement("root", "A & B"));
$written = $writer->flush();
$contents = file_get_contents($path);
var_dump($written === strlen($contents));
echo $contents, "\n";
unlink($path);

$path = tempnam(sys_get_temp_dir(), "phrust_xmlwriter_uri_");
$writer = XMLWriter::toUri($path);
var_dump($writer instanceof XMLWriter);
var_dump($writer->startElement("root"));
var_dump($writer->writeCdata("<raw>"));
var_dump($writer->endElement());
$written = $writer->flush(false);
$contents = file_get_contents($path);
var_dump($written === strlen($contents));
echo $contents, "\n";
unlink($path);

$path = tempnam(sys_get_temp_dir(), "phrust_xmlwriter_uri_");
$writer = xmlwriter_open_uri($path);
var_dump($writer instanceof XMLWriter);
var_dump(xmlwriter_start_element($writer, "root"));
var_dump(xmlwriter_write_comment($writer, "note"));
var_dump(xmlwriter_write_element($writer, "child", "C"));
var_dump(xmlwriter_end_document($writer));
$written = xmlwriter_flush($writer);
$contents = file_get_contents($path);
var_dump($written === strlen($contents));
echo $contents, "\n";
unlink($path);
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
<root>A &amp; B</root>
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
<root><![CDATA[<raw>]]></root>
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
<root><!--note--><child>C</child></root>

--TEST--
xml: selected constants and parser option retention
--DESCRIPTION--
Generated XML coverage for selected constant values, function visibility, and parse-huge option retention.
--EXTENSIONS--
xml
--FILE--
<?php
var_dump(XML_OPTION_PARSE_HUGE);
var_dump(XML_SAX_IMPL);
var_dump(function_exists('xml_get_error_code'));
var_dump(function_exists('xml_error_string'));
var_dump(function_exists('xml_parser_free'));

$parser = xml_parser_create();
var_dump(xml_parser_get_option($parser, XML_OPTION_PARSE_HUGE));
var_dump(xml_parser_set_option($parser, XML_OPTION_PARSE_HUGE, true));
var_dump(xml_parser_get_option($parser, XML_OPTION_PARSE_HUGE));
var_dump(xml_parser_set_option($parser, XML_OPTION_PARSE_HUGE, false));
var_dump(xml_parser_get_option($parser, XML_OPTION_PARSE_HUGE));
?>
--EXPECT--
int(5)
string(6) "libxml"
bool(true)
bool(true)
bool(true)
bool(false)
bool(true)
bool(true)
bool(true)
bool(false)

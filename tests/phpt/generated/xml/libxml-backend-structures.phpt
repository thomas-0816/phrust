--TEST--
xml: libxml backend parses declarations, namespaces, comments, and CDATA
--DESCRIPTION--
Generated XML coverage for libxml-backed parsing projected into xml_parse_into_struct.
--EXTENSIONS--
xml
--FILE--
<?php
$parser = xml_parser_create();
xml_parser_set_option($parser, XML_OPTION_CASE_FOLDING, false);
$xml = '<?xml version="1.0"?><root xmlns:h="urn:h" id="7"><!--note--><h:item a="b"><![CDATA[A & B]]></h:item></root>';
$values = array();
$index = array();
var_dump(xml_parse_into_struct($parser, $xml, $values, $index));
print_r($index);
print_r($values);
?>
--EXPECT--
int(1)
Array
(
    [root] => Array
        (
            [0] => 0
            [1] => 2
        )

    [h:item] => Array
        (
            [0] => 1
        )

)
Array
(
    [0] => Array
        (
            [tag] => root
            [type] => open
            [level] => 1
            [attributes] => Array
                (
                    [xmlns:h] => urn:h
                    [id] => 7
                )

        )

    [1] => Array
        (
            [tag] => h:item
            [type] => complete
            [level] => 2
            [attributes] => Array
                (
                    [a] => b
                )

            [value] => A & B
        )

    [2] => Array
        (
            [tag] => root
            [type] => close
            [level] => 1
        )

)

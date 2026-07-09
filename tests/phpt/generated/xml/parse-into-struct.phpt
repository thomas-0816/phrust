--TEST--
xml: selected xml_parse_into_struct flattening over the strict parser MVP
--DESCRIPTION--
Generated XML coverage for parse-into-struct values/index arrays and case folding.
--EXTENSIONS--
xml
--FILE--
<?php
$parser = xml_parser_create();
var_dump(xml_parse_into_struct($parser, '<root a="b">hi<child/>there</root>', $values, $index));
print_r($index);
print_r($values);

$parser = xml_parser_create();
xml_parser_set_option($parser, XML_OPTION_CASE_FOLDING, false);
var_dump(xml_parse_into_struct($parser, '<root a="b"><child>text</child></root>', $values, $index));
print_r($index);
print_r($values);
?>
--EXPECT--
int(1)
Array
(
    [ROOT] => Array
        (
            [0] => 0
            [1] => 2
            [2] => 3
        )

    [CHILD] => Array
        (
            [0] => 1
        )

)
Array
(
    [0] => Array
        (
            [tag] => ROOT
            [type] => open
            [level] => 1
            [attributes] => Array
                (
                    [A] => b
                )

            [value] => hi
        )

    [1] => Array
        (
            [tag] => CHILD
            [type] => complete
            [level] => 2
        )

    [2] => Array
        (
            [tag] => ROOT
            [value] => there
            [type] => cdata
            [level] => 1
        )

    [3] => Array
        (
            [tag] => ROOT
            [type] => close
            [level] => 1
        )

)
int(1)
Array
(
    [root] => Array
        (
            [0] => 0
            [1] => 2
        )

    [child] => Array
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
                    [a] => b
                )

        )

    [1] => Array
        (
            [tag] => child
            [type] => complete
            [level] => 2
            [value] => text
        )

    [2] => Array
        (
            [tag] => root
            [type] => close
            [level] => 1
        )

)

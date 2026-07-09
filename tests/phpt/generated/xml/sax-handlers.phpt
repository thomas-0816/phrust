--TEST--
xml: selected SAX handlers dispatch over the strict parser MVP
--DESCRIPTION--
Generated XML SAX coverage for element, character data, default handlers, and case folding.
--EXTENSIONS--
xml
--FILE--
<?php
$parser = xml_parser_create();
var_dump(xml_set_element_handler($parser, 'xml_start', 'xml_finish'));
var_dump(xml_set_character_data_handler($parser, 'xml_text'));

function xml_start($parser, $name, $attrs) {
    echo "S:$name:" . json_encode($attrs) . "\n";
}

function xml_finish($parser, $name) {
    echo "E:$name\n";
}

function xml_text($parser, $data) {
    echo "T:$data\n";
}

var_dump(xml_parse($parser, '<root a="b">hi<child/>there</root>', true));

$parser = xml_parser_create();
xml_parser_set_option($parser, XML_OPTION_CASE_FOLDING, false);
xml_set_element_handler(
    $parser,
    function ($parser, $name, $attrs) {
        echo "s:$name:" . json_encode($attrs) . "\n";
    },
    function ($parser, $name) {
        echo "e:$name\n";
    }
);
xml_parse($parser, '<root a="b"><child/></root>', true);

$parser = xml_parser_create();
xml_set_default_handler($parser, function ($parser, $data) {
    echo "D:$data\n";
});
var_dump(xml_parse($parser, '<root a="b">hi<child/>there</root>', true));
?>
--EXPECT--
bool(true)
bool(true)
S:ROOT:{"A":"b"}
T:hi
S:CHILD:[]
E:CHILD
T:there
E:ROOT
int(1)
s:root:{"a":"b"}
s:child:[]
e:child
e:root
D:<root a="b">
D:hi
D:<child>
D:</child>
D:there
D:</root>
int(1)

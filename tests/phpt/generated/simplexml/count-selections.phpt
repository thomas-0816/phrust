--TEST--
simplexml: count root, child selections, and children lists
--DESCRIPTION--
Generated SimpleXML coverage for PHP count() behavior across document roots,
missing child selections, single child selections, duplicate child selections,
and children() on a selection list.
--EXTENSIONS--
simplexml
--FILE--
<?php
$xml = simplexml_load_string('<root><outer><x>X</x><y>Y</y></outer><multi><z>Z1</z></multi><multi><z>Z2</z><q>Q2</q></multi></root>');

echo "root=", count($xml), "\n";
echo "missing=", count($xml->missing), " str=[", $xml->missing, "]\n";
echo "outer=", count($xml->outer), "\n";
echo "outer children=", count($xml->outer->children()), "\n";
foreach ($xml->outer as $name => $value) {
    echo "outer iter ", $name, "=", $value, " count=", count($value), "\n";
}
echo "multi=", count($xml->multi), "\n";
echo "multi children=", count($xml->multi->children()), "\n";

foreach ($xml->multi as $name => $value) {
    echo "multi iter ", $name, " count=", count($value), " children=", count($value->children()), "\n";
}
?>
--EXPECT--
root=3
missing=0 str=[]
outer=1
outer children=2
outer iter outer= count=2
multi=2
multi children=1
multi iter multi count=1 children=1
multi iter multi count=2 children=2

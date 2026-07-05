--TEST--
simplexml: array offsets, attributes, and getName
--DESCRIPTION--
Generated SimpleXML coverage for PHP-style attribute offsets, numeric child
selection, attribute iteration, and getName() metadata.
--EXTENSIONS--
simplexml
--FILE--
<?php
$xml = simplexml_load_string('<root id="7" code="x"><item code="a">A</item><item code="b">B</item></root>');
echo "root attr=", $xml['id'], "\n";
var_dump($xml['missing']);
var_dump($xml['item']);
echo "item0=", $xml->item[0], " name=", $xml->item[0]->getName(), " count=", count($xml->item[0]), "\n";
echo "item1=", $xml->item[1], " name=", $xml->item[1]->getName(), " count=", count($xml->item[1]), "\n";
var_dump($xml->item[2]);
echo "item0 attr=", $xml->item[0]['code'], "\n";
var_dump($xml->item[0]['missing']);
$attrs = $xml->attributes();
echo "attrs=", $attrs, " name=", $attrs->getName(), " count=", count($attrs), "\n";
foreach ($attrs as $name => $value) {
    echo "attr ", $name, "=", $value, " name=", $value->getName(), " count=", count($value), "\n";
}
echo "attrs id dim=", $attrs['id'], "\n";
echo "attrs zero=", $attrs[0], " name=", $attrs[0]->getName(), "\n";
var_dump($attrs[99]);
echo "missing child name=[", $xml->missing->getName(), "] count=", count($xml->missing), " string=[", $xml->missing, "]\n";
?>
--EXPECT--
root attr=7
NULL
NULL
item0=A name=item count=0
item1=B name=item count=0
NULL
item0 attr=a
NULL
attrs=7 name=id count=2
attr id=7 name=id count=0
attr code=x name=code count=0
attrs id dim=7
attrs zero=7 name=id
NULL
missing child name=[] count=0 string=[]

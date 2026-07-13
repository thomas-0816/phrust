--TEST--
dom: DOMXPath, DOMNamedNodeMap, namespace queries, and file load/save
--DESCRIPTION--
Generated DOM coverage for app-facing DOMXPath queries, named-node-map access,
namespace-aware element selection, and DOMDocument load/save file methods.
--EXTENSIONS--
dom
--FILE--
<?php
var_dump(class_exists("DOMNamedNodeMap", false));
var_dump(class_exists("DOMXPath", false));
$xml = '<root xmlns:h="urn:h" id="7" code="x"><h:item>A</h:item><h:item>B</h:item></root>';
$document = new DOMDocument();
var_dump($document->loadXML($xml));
$root = $document->documentElement;
echo "attrs=", $root->attributes->length, "|", $root->attributes->item(0)->name, "|", $root->attributes->getNamedItem("code")->value, "\n";
echo "attr=", ($root->hasAttribute("id") ? "yes" : "no"), "|", $root->getAttributeNode("id")->value, "|", $root->getAttribute("xmlns:h"), "\n";
$root->removeAttribute("code");
echo "removed=", ($root->hasAttribute("code") ? "no" : "yes"), "|", $root->attributes->length, "\n";
$items = $document->getElementsByTagNameNS("urn:h", "item");
echo "ns=", $items->length, "|", $items->item(1)->nodeName, "|", $items->item(1)->textContent, "\n";
$xpath = new DOMXPath($document);
$nodes = $xpath->query("//h:item");
echo "xpath=", $nodes->length, "|", $nodes->item(0)->nodeName, "|", $nodes->item(0)->textContent, "\n";
echo "eval=", $xpath->evaluate("string(/root/h:item[2])"), "\n";
$path = tempnam(sys_get_temp_dir(), "dom");
var_dump($document->save($path) !== false);
$roundtrip = new DOMDocument();
var_dump($roundtrip->load($path));
echo "file=", $roundtrip->documentElement->tagName, "|", $roundtrip->getElementsByTagName("h:item")->length, "\n";
@unlink($path);
?>
--EXPECT--
bool(true)
bool(true)
bool(true)
attrs=2|id|x
attr=yes|7|urn:h
removed=yes|1
ns=2|h:item|B
xpath=2|h:item|A
eval=B
bool(true)
bool(true)
file=root|2

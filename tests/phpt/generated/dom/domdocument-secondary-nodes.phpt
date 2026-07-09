--TEST--
dom: DOMDocument secondary node MVP
--DESCRIPTION--
Generated DOM coverage for bounded DOMAttr, DOMComment, and DOMCdataSection nodes.
--EXTENSIONS--
dom
--FILE--
<?php
$document = new DOMDocument();
$root = $document->createElement('root');
$attribute = $document->createAttribute('kind');
$attribute->value = 'demo';
var_dump($attribute instanceof DOMAttr);
var_dump($root->setAttributeNode($attribute) instanceof DOMAttr);
$comment = $document->createComment('note');
$cdata = $document->createCDATASection('A < B & C');
var_dump($comment instanceof DOMComment);
var_dump($cdata instanceof DOMCdataSection);
echo $comment->nodeName, "|", $comment->nodeValue, "\n";
echo $cdata->nodeName, "|", $cdata->textContent, "\n";
$root->appendChild($comment);
$root->appendChild($cdata);
$document->appendChild($root);
echo $document->saveXML(), "\n";
echo $document->textContent, "\n";
?>
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(true)
#comment|note
#cdata-section|A < B & C
<root kind="demo"><!--note--><![CDATA[A < B & C]]></root>
A < B & C

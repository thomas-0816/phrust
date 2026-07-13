--TEST--
phar: ArrayAccess returns PharFileInfo with metadata
--DESCRIPTION--
Generated PHAR coverage for read-only ArrayAccess, PharFileInfo content, and
archive/entry metadata decoding from the manifest.
--EXTENSIONS--
phar
--FILE--
<?php
$path = __DIR__ . '/metadata-fixture.phar';
$hex = '3c3f706870205f5f48414c545f434f4d50494c455228293b203f3e0d0abc00000002000000110000000100150000006d657461646174612d666978747572652e706861722b000000613a323a7b733a373a2261726368697665223b733a343a226d657461223b733a313a226e223b693a333b7d08000000646174612e74787407000000f93c506a07000000156a2c42a40100001d000000613a313a7b733a353a22656e747279223b733a343a226d657461223b7d0d0000006c69622f68656c6c6f2e7068702e000000f93c506a2e000000924eee49a4010000000000007061796c6f61643c3f706870206563686f202266726f6d2d706861727c223b2072657475726e2022696e636c7564652d6f6b223b0a84e76fd65c15ed5859574cf7d652aafa41b0259dc96873783289da7164e0dd0c0300000047424d42';
file_put_contents($path, hex2bin($hex));

$archive = new Phar($path);
$entry = $archive['data.txt'];
var_dump($archive instanceof ArrayAccess);
var_dump($archive instanceof Countable);
var_dump($entry instanceof PharFileInfo);
var_dump($entry instanceof SplFileInfo);
var_dump($archive->getMetadata());
var_dump($entry->getMetadata());
var_dump($entry->getContent());
var_dump($entry->getFilename());
echo str_replace('phar://' . $path . '/', '', $entry->getPathname()), "\n";
unlink($path);
?>
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(true)
array(2) {
  ["archive"]=>
  string(4) "meta"
  ["n"]=>
  int(3)
}
array(1) {
  ["entry"]=>
  string(4) "meta"
}
string(7) "payload"
string(8) "data.txt"
data.txt

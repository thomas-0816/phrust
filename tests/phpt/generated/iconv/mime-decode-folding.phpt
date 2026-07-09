--TEST--
iconv_mime_decode selected folding and charset suffixes
--SKIPIF--
<?php if (!extension_loaded("iconv")) die("skip iconv extension not loaded"); ?>
--FILE--
<?php
$folded = "Subject: =?ISO-8859-1?Q?Pr=FCfung?=\n"
    . "    =?ISO-8859-1*de_DE?Q?Pr=FCfung?=\t\n"
    . "     =?ISO-8859-2?Q?k=F9=D4=F1=D3let?=";
$decoded = iconv_mime_decode($folded, 0, "UTF-8");
var_dump(iconv_strlen($decoded, "UTF-8"));
var_dump(bin2hex($decoded));

$inline = "Subject: =?ISO-8859-1?Q?Pr=FCfung?= =?ISO-8859-1*de_DE?Q?=20Pr=FCfung?= \t  =?ISO-8859-2?Q?k=F9=D4=F1=D3let?=";
$decoded = iconv_mime_decode($inline, 0, "UTF-8");
var_dump(iconv_strlen($decoded, "UTF-8"));
var_dump(bin2hex($decoded));
?>
--EXPECT--
int(31)
string(74) "5375626a6563743a205072c3bc66756e675072c3bc66756e676bc5afc394c584c3936c6574"
int(32)
string(76) "5375626a6563743a205072c3bc66756e67205072c3bc66756e676bc5afc394c584c3936c6574"

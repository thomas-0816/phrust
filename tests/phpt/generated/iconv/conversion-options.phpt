--TEST--
iconv selected //IGNORE and //TRANSLIT options
--SKIPIF--
<?php if (!extension_loaded("iconv")) die("skip iconv extension not loaded"); ?>
--FILE--
<?php
var_dump(bin2hex(iconv("UTF-8", "ASCII//IGNORE", "Pr\xC3\xBCfung \xE2\x82\xAC")));
var_dump(bin2hex(iconv("UTF-8", "ASCII//TRANSLIT", "Pr\xC3\xBCfung \xE2\x82\xAC")));
var_dump(bin2hex(iconv("UTF-8", "ISO-8859-1//IGNORE", "Price \xE2\x82\xAC")));
var_dump(bin2hex(iconv("UTF-8", "ISO-8859-1//TRANSLIT", "Price \xE2\x82\xAC")));
?>
--EXPECT--
string(14) "507266756e6720"
string(24) "5072227566756e6720455552"
string(12) "507269636520"
string(18) "507269636520455552"

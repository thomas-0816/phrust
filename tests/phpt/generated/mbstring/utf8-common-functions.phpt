--TEST--
mbstring: bounded UTF-8 common functions
--DESCRIPTION--
Focused mbstring UTF-8 coverage for length, substring, and case
conversion. Reference output captured from PHP 8.5.7 php-src with
--enable-mbstring --disable-mbregex.
--FILE--
<?php
var_dump(mb_strlen("Aé日", "UTF-8"));
var_dump(mb_substr("Aé日", 1, 2, "UTF-8"));
var_dump(mb_substr("Aé日", -2, 1, "UTF-8"));
var_dump(mb_strtolower("ÄÖÜ İ ABC", "UTF-8"));
var_dump(mb_strtoupper("äöü ß abc", "UTF-8"));
?>
--EXPECT--
int(3)
string(5) "é日"
string(2) "é"
string(14) "äöü i̇ abc"
string(13) "ÄÖÜ SS ABC"

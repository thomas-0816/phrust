--TEST--
intl: bounded normalizer, grapheme, and transliterator behavior
--DESCRIPTION--
Generated intl MVP coverage for bounded NFC, character slicing, and Latin ASCII transliteration.
--EXTENSIONS--
intl
--FILE--
<?php
var_dump(Normalizer::isNormalized("abc"));
var_dump(Normalizer::normalize("abc"));
echo grapheme_strlen("hé"), "\n";
echo grapheme_substr("héllo", 1, 3), "\n";
echo transliterator_transliterate("Latin-ASCII", "Héllo"), "\n";
echo intl_get_error_code(), "\n";
?>
--EXPECT--
bool(true)
string(3) "abc"
2
éll
Hello
0

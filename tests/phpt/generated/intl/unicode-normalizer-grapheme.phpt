--TEST--
intl: Unicode normalization forms and grapheme clusters
--DESCRIPTION--
Generated intl coverage for Unicode Normalizer forms and extended grapheme cluster operations.
--EXTENSIONS--
intl
--FILE--
<?php
var_dump(function_exists("grapheme_strpos"));
var_dump(function_exists("grapheme_stripos"));
var_dump(Normalizer::FORM_D);
var_dump(Normalizer::FORM_KD);
var_dump(Normalizer::FORM_C);
var_dump(Normalizer::FORM_KC);

$decomposed = "e\u{0301}";
var_dump(Normalizer::normalize($decomposed, Normalizer::FORM_C));
var_dump(Normalizer::isNormalized($decomposed, Normalizer::FORM_C));
echo bin2hex(normalizer_normalize("é", Normalizer::FORM_D)), "\n";
var_dump(normalizer_normalize("①", Normalizer::FORM_KC));

$family = "👨‍👩‍👧‍👦";
$flag = "🇩🇪";
$skin = "👍🏽";
echo grapheme_strlen($family), "|", grapheme_strlen($flag), "|", grapheme_strlen($skin), "\n";
$value = "a{$family}b";
echo grapheme_strlen($value), "\n";
echo grapheme_substr($value, 1, 1), "\n";
var_dump(grapheme_strpos($value, "b"));
var_dump(grapheme_strpos($value, "b", 3));
var_dump(grapheme_stripos("Äbc", "ä"));
?>
--EXPECT--
bool(true)
bool(true)
int(4)
int(8)
int(16)
int(32)
string(2) "é"
bool(false)
65cc81
string(1) "1"
1|1|1
3
👨‍👩‍👧‍👦
int(2)
bool(false)
int(0)

--TEST--
intl: common bounded symbols are available
--DESCRIPTION--
Focused intl coverage for the bounded helper surface.
--EXTENSIONS--
intl
--FILE--
<?php
foreach ([
    "intl_get_error_code",
    "grapheme_strlen",
    "normalizer_normalize",
] as $function) {
    echo $function, function_exists($function) ? " available\n" : " unavailable\n";
}

foreach ([
    "Locale",
    "NumberFormatter",
    "Collator",
    "IntlChar",
    "Normalizer",
] as $class) {
    echo $class, class_exists($class) ? " available\n" : " unavailable\n";
}
?>
--EXPECT--
intl_get_error_code available
grapheme_strlen available
normalizer_normalize available
Locale available
NumberFormatter available
Collator available
IntlChar available
Normalizer available

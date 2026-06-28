--TEST--
intl: common symbols are guarded while extension is unavailable
--DESCRIPTION--
Focused intl stub coverage that keeps common ICU-backed symbols unavailable.
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
] as $class) {
    echo $class, class_exists($class) ? " available\n" : " unavailable\n";
}
?>
--EXPECT--
intl_get_error_code unavailable
grapheme_strlen unavailable
normalizer_normalize unavailable
Locale unavailable
NumberFormatter unavailable
Collator unavailable
IntlChar unavailable

<?php
// runtime-semantics: expect=pass regression_category=objects reference_behavior=stdout:Categories|category regression_case=lazy-native-compilation

require __DIR__ . '/_data/lazy-cross-unit-property-function.php';
require __DIR__ . '/_data/lazy-cross-unit-property-class.php';
require __DIR__ . '/_data/lazy-cross-unit-property-hooks.php';

$taxonomy = new LazyCrossUnitTaxonomy('category');
echo $taxonomy->labels->name, '|', $taxonomy->name, "\n";

<?php
// runtime-semantics: expect=pass regression_category=types reference_behavior=stdout:string: regression_case=lazy-native-compilation
declare(strict_types=1);

require __DIR__ . '/_data/lazy-cross-unit-cast-provider.php';
require __DIR__ . '/_data/lazy-cross-unit-cast-target.php';

function lazy_cross_unit_cast_caller($target) {
    foreach (array('path.mo') as $file) {
        $file = (string) lazy_cross_unit_return_false('hook', $file, 'domain', 'locale');
        $target->mutate($file);
        return gettype($file) . ':' . $file;
    }
}

echo lazy_cross_unit_cast_caller(new LazyCrossUnitCastTarget()), "\n";

<?php
class StaticPropertyMagicConstantArrayDefault
{
    public static $files = array(
        'polyfill' => __DIR__ . '/../include_eval_autoload/_data/once-shared.php',
    );
}

var_dump(is_array(StaticPropertyMagicConstantArrayDefault::$files));
var_dump(array_keys(StaticPropertyMagicConstantArrayDefault::$files));
echo basename(StaticPropertyMagicConstantArrayDefault::$files['polyfill']), "\n";

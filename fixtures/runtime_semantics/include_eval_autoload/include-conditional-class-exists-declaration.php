<?php
require __DIR__ . '/_data/conditional-class-exists-child.php';

var_dump(class_exists('ConditionalIncludeFirst', false));
var_dump(class_exists('ConditionalIncludeSecond', false));
echo (new ConditionalIncludeSecond())->value(), "\n";

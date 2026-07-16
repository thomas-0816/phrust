<?php

require __DIR__ . '/_data/once-child-a.php';
require __DIR__ . '/_data/once-child-b.php';

var_dump(class_exists('NativeIncludeOnceSharedClass', false));

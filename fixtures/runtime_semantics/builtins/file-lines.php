<?php

$path = __DIR__ . '/../_data/file-lines.txt';
var_export(file($path));
echo "\n";
var_export(file($path, FILE_IGNORE_NEW_LINES | FILE_SKIP_EMPTY_LINES));
echo "\n";

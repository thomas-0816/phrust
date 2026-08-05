<?php

$base = "target/mkdir-native-fixture";
$deep = $base . "/nested";

if (is_dir($deep)) {
    rmdir($deep);
}
if (is_dir($base)) {
    rmdir($base);
}

var_dump(mkdir($deep, 0750, true));
echo is_dir($deep) ? "recursive\n" : "missing\n";
var_dump(rmdir($deep));
var_dump(rmdir($base));

$context = stream_context_create();
var_dump(mkdir($base, 0700, false, $context));
echo is_dir($base) ? "context\n" : "missing\n";
var_dump(rmdir($base));

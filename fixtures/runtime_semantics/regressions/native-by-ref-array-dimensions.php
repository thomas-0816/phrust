<?php

function increment_native_array_dimension(&$value) {
    $value++;
}

$nested = [[1]];
increment_native_array_dimension($nested[0][0]);
echo "nested:", $nested[0][0], "\n";

$missing = [];
increment_native_array_dimension($missing['created']);
echo "missing:", $missing['created'], "\n";

$aliased = 1;
$references = [&$aliased];
increment_native_array_dimension($references[0]);
echo "alias:", $aliased, ":", $references[0], "\n";

$copy_on_write = [1];
$snapshot = $copy_on_write;
increment_native_array_dimension($copy_on_write[0]);
echo "cow:", $copy_on_write[0], ":", $snapshot[0], "\n";

$strings = ['key' => 1];
increment_native_array_dimension($strings['key']);
echo "string:", $strings['key'], "\n";

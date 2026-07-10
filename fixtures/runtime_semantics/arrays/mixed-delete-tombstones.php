<?php
// runtime-semantics: expect=pass
$array = [];
for ($index = 0; $index < 128; $index++) {
    $array["key-" . $index] = $index;
}

// Deterministic pseudo-random deletion and reinsertion sequence. This crosses
// the runtime's compaction threshold without making that threshold observable.
$state = 17;
for ($step = 0; $step < 192; $step++) {
    $state = ($state * 73 + 19) % 128;
    $key = "key-" . $state;
    unset($array[$key]);
    if (($step % 3) === 0) {
        $array[$key] = $step;
    }
}

$copy = $array;
unset($copy["key-127"]);
$copy[500] = "sparse";
$copy[] = "append";
$reference =& $copy["key-64"];
$reference = "reference";

echo count($array), "|", count($copy), "|";
$shown = 0;
foreach ($copy as $key => $value) {
    if ($shown++ === 8) {
        break;
    }
    echo $key, "=", $value, ";";
}
echo "|", isset($array[500]) ? "bad" : "cow", "|", $copy["key-64"];

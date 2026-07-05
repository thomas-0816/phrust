<?php
// instanceof paths shared by dense and rich execution: class hierarchies,
// interfaces, non-objects, unknown classes (no autoload), and closures.

interface ProbeMarker {}
class ProbeBase {}
class ProbeChild extends ProbeBase implements ProbeMarker {}

function check($value, $label) {
    echo $label, ":";
    echo $value instanceof ProbeBase ? "B" : "-";
    echo $value instanceof ProbeChild ? "C" : "-";
    echo $value instanceof ProbeMarker ? "M" : "-";
    echo $value instanceof Traversable ? "T" : "-";
    echo $value instanceof UndeclaredProbeClass ? "U" : "-";
    echo "\n";
}

check(new ProbeBase(), "base");
check(new ProbeChild(), "child");
check(42, "int");
check("ProbeBase", "string");
check(null, "null");
check([1, 2], "array");
check(function () {}, "closure");
check(new ArrayIterator([1]), "iterator");

$closure = function () {};
var_dump($closure instanceof Closure);

// instanceof inside a loop keeps working after the class table warms.
$hits = 0;
$values = [new ProbeChild(), new ProbeBase(), 7, new ProbeChild()];
foreach ($values as $value) {
    if ($value instanceof ProbeMarker) {
        $hits++;
    }
}
echo "marker-hits:", $hits, "\n";

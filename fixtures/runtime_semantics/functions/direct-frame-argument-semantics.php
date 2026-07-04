<?php
// runtime-semantics: category=functions expect=pass php_ref_required=1
// Direct-frame calls must not change argument semantics: func_get_args
// observation (including extras and defaults), backtrace arguments from
// nested throws, named args, variadics, by-ref params, closures.
function observer($a, $b = 5) {
    return implode(",", func_get_args()) . "#" . func_num_args();
}
echo observer(1), "|", observer(1, 2), "|", observer(1, 2, 3), "\n";

function thrower($x, $y) {
    throw new RuntimeException("boom:$x:$y");
}
function middle($v) {
    thrower($v, $v * 2);
}
try {
    middle(9);
} catch (RuntimeException $e) {
    $frame = $e->getTrace()[0];
    echo $e->getMessage(), "|", $frame["function"], "|", implode(",", $frame["args"] ?? []), "\n";
}

function named($first, $second = "d2", $third = "d3") {
    return "$first/$second/$third";
}
echo named(1, third: "T"), "\n";

function spread(...$parts) {
    return count($parts) . ":" . implode("-", $parts);
}
echo spread("a", "b", "c"), "\n";

function bump(&$n, $by) {
    $n += $by;
}
$counter = 10;
bump($counter, 5);
echo $counter, "\n";

$mul = function ($a, $b) {
    return $a * $b;
};
$outer = 3;
$capturing = function ($x) use ($outer) {
    return $x + $outer;
};
echo $mul(6, 7), "|", $capturing(4), "\n";

class Builder {
    private $parts = [];
    public function add($part) {
        $this->parts[] = $part;
        return $this;
    }
    public function join() {
        return implode("+", $this->parts);
    }
}
echo (new Builder())->add("x")->add("y")->join(), "\n";

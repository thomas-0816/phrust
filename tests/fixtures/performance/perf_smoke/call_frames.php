<?php
function tiny_frame_add($a, $b) { return $a + $b; }
function call_context_frame($a) { return func_num_args() . ":" . count(func_get_args()); }
function named_frame($a, $b = 2) { return $a + $b; }
function variadic_frame(...$xs) { return count($xs); }
function byref_frame(&$x) { $x++; }
function gen_frame() { yield 1; }

class PerfFrameLayoutService {
    public function inc($x) { return $x + 1; }
}

$sum = 0;
for ($i = 0; $i < 12; $i++) {
    $sum = tiny_frame_add($sum, 1);
}
echo "tiny:", $sum, "\n";
echo "context:", call_context_frame(1, 2), "\n";

$service = new PerfFrameLayoutService();
for ($i = 0; $i < 3; $i++) {
    echo "method:", $service->inc($i), "\n";
}

$base = 3;
$closure = function ($x) use ($base) { return $x + $base; };
echo "closure:", $closure(4), "\n";
echo "named:", named_frame(b: 5, a: 4), "\n";
echo "variadic:", variadic_frame(1, 2, 3), "\n";

$value = 1;
byref_frame($value);
echo "byref:", $value, "\n";

$generator = gen_frame();
echo "gen:", $generator->current(), "\n";

$fiber = new Fiber(function () { Fiber::suspend("fiber"); });
echo "fiber:", $fiber->start(), "\n";

require "tests/fixtures/performance/perf_smoke/_support/call_frame_include.php";
eval('echo "eval:5\n";');
echo "dynamic:", call_user_func("tiny_frame_add", 2, 3), "\n";

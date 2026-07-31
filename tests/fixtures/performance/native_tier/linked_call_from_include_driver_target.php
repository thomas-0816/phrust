<?php
echo perf_cross_unit_literal(), "\n";
echo call_user_func('perf_cross_unit_argument', 'callback'), "\n";
echo call_user_func_array('perf_cross_unit_argument', ['array']), "\n";
echo implode(',', array_map('perf_cross_unit_argument', ['map-a', 'map-b'])), "\n";
echo implode(',', array_filter(['', 'filter'], 'perf_cross_unit_argument')), "\n";
echo array_reduce([1, 2, 3], 'perf_cross_unit_sum', 10), "\n";
$perf_cross_unit_typed_calls = 0;
try {
    array_map('perf_cross_unit_typed_count', [1, 'not-an-int', 3]);
} catch (TypeError $error) {
}
echo $perf_cross_unit_typed_calls, "\n";
$perf_cross_unit_closure = perf_cross_unit_closure_factory(5);
echo $perf_cross_unit_closure(7), "\n";

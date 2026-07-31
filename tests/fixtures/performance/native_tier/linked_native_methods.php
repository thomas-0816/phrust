<?php

require __DIR__ . '/linked_native_methods_target.php';

function perf_linked_static_method(int $value): int
{
    return PerfLinkedNativeMethods::twice($value);
}

function perf_linked_static_reference(&$value): int
{
    return PerfLinkedNativeMethods::increment($value);
}

function perf_linked_reference_method(): string
{
    $object = new PerfLinkedNativeMethods(7);
    $value = 20;
    $incremented = $object->incrementInstance($value);
    $slot =& $object->slot();
    $slot = 23;
    return $incremented . '|' . $value . '|' . $object->readSlot();
}

function perf_linked_static_method_callbacks(): string
{
    $mapped = array_map('PerfLinkedNativeMethods::twice', [1, 2, 3]);
    $called = call_user_func('PerfLinkedNativeMethods::twice', 5);
    $calledArray = call_user_func_array('PerfLinkedNativeMethods::twice', [6]);
    $arrayMapped = array_map(
        ['PerfLinkedNativeMethods', 'twice'],
        [2, 3],
    );
    $arrayCalled = call_user_func(['PerfLinkedNativeMethods', 'twice'], 7);
    $arrayCalledArgs = call_user_func_array(
        ['PerfLinkedNativeMethods', 'twice'],
        [8],
    );
    $staticCallable = ['PerfLinkedNativeMethods', 'twice'];
    $invoked = $staticCallable(9);
    return implode(',', $mapped)
        . '|' . $called
        . '|' . $calledArray
        . '|' . implode(',', $arrayMapped)
        . '|' . $arrayCalled
        . '|' . $arrayCalledArgs
        . '|' . $invoked;
}

function perf_linked_instance_method_callbacks(): string
{
    $object = new PerfLinkedNativeMethods(10);
    $mapped = array_map([$object, 'add'], [1, 2, 3]);
    $called = call_user_func([$object, 'add'], 4);
    $calledArray = call_user_func_array([$object, 'add'], [5]);
    $instanceCallable = [$object, 'add'];
    $invoked = $instanceCallable(6);
    return implode(',', $mapped)
        . '|' . $called
        . '|' . $calledArray
        . '|' . $invoked;
}

$object = new PerfLinkedNativeMethods(7);
$plain = new PerfLinkedPlainMethods();
$value = 10;
echo $object->add(4), '|';
echo perf_linked_static_method(6), '|';
echo perf_linked_static_reference($value), '|', $value, '|';
echo $plain->value(), '|';
echo perf_linked_reference_method(), '|';
echo perf_linked_static_method_callbacks(), '|';
echo perf_linked_instance_method_callbacks(), '|';

$child = new PerfLinkedOverrideChild();
echo $child->value(), '|', $child->expose(), '|';
try {
    $child->hidden();
} catch (Error $error) {
    echo 'visibility';
}

$magic = new PerfLinkedMagicMethods();
echo '|', $magic->missing(4), "\n";

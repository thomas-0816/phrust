<?php
// runtime-semantics: requires_ref_extension=filter php_ref_optional_reason=reference-build-lacks-filter

function run_exact_filter(): array
{
    $source = [
        'age' => '42',
        'email' => ' person @example.com ',
    ];

    return [
        filter_var($source['age'], 257),
        filter_var($source['email'], 517),
        filter_var_array($source, 516, false),
        filter_id('int'),
        count(filter_list()),
        filter_has_var(1, 'not-present'),
        filter_input(1, 'not-present', 257, 134217728),
        filter_input_array(1, 516, false),
    ];
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = run_exact_filter();
}
var_dump($result);

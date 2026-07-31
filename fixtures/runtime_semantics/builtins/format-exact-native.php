<?php

function run_exact_format(): array
{
    $formatted = sprintf('%s:%04d:%.1f', 'item', 7, 2.5);
    $vectorFormatted = vsprintf('%2$s/%1$03d', [5, 'value']);
    $printed = printf('[%s=%d]', 'count', 3);
    $vectorPrinted = vprintf('<%s:%02d>\n', ['id', 4]);
    $wideFormatted = sprintf(
        '%s:%s:%s:%s:%s:%s:%s',
        'one',
        'two',
        'three',
        'four',
        'five',
        'six',
        'seven',
    );
    $widePrinted = printf(
        '{%s,%s,%s,%s,%s,%s,%s}',
        'one',
        'two',
        'three',
        'four',
        'five',
        'six',
        'seven',
    );
    $integerFormatted = sprintf('%d', INF);
    $castedInfinity = (int) INF;
    $castedLargeFloat = (int) 1.0e30;
    $groupedNumber = number_format(1234567.875, 2, ',', '.');
    $numericString = number_format('1234.5', 3, '.', '');
    $defaultNumber = number_format(1234);

    return [
        $formatted,
        $vectorFormatted,
        $printed,
        $vectorPrinted,
        $wideFormatted,
        $widePrinted,
        $integerFormatted,
        $castedInfinity,
        $castedLargeFloat,
        $groupedNumber,
        $numericString,
        $defaultNumber,
    ];
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = run_exact_format();
}
var_dump($result);

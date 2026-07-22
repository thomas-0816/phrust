<?php

function run_exact_pcre(): array
{
    $matches = null;
    $matched = preg_match('/(ca)(t)/', 'cat', $matches);

    $all = null;
    $matchedAll = preg_match_all('/a./', 'ab ac', $all);

    $count = 0;
    $replaced = preg_replace('/a/', 'A', 'banana', -1, $count);

    $filteredCount = 0;
    $filtered = preg_filter('/a/', 'A', ['cat', 'dog'], -1, $filteredCount);

    $typedError = null;
    try {
        preg_quote([]);
    } catch (Throwable $error) {
        $typedError = get_class($error) . ': ' . $error->getMessage();
    }

    return [
        $matched,
        $matches,
        $matchedAll,
        $all,
        $replaced,
        $count,
        $filtered,
        $filteredCount,
        preg_split('/,+/', 'one,two,,three'),
        preg_grep('/a/', ['cat', 'dog', 'ant']),
        preg_quote('a.b/c', '/'),
        preg_last_error(),
        preg_last_error_msg(),
        $typedError,
    ];
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = run_exact_pcre();
}
var_dump($result);

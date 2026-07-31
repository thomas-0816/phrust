<?php

function run_exact_pcre(): array
{
    $matches = null;
    $matched = preg_match('/(ca)(t)/', 'cat', $matches);

    $all = null;
    $matchedAll = preg_match_all('/a./', 'ab ac', $all);

    $setOrder = null;
    $matchedSetOrder = preg_match_all(
        '/(a)(.)/',
        'ab ac',
        $setOrder,
        PREG_SET_ORDER | PREG_OFFSET_CAPTURE,
    );

    $named = null;
    $matchedNamed = preg_match(
        '/(?<word>[a-z]+)(?<missing>z)?/',
        'native',
        $named,
        PREG_UNMATCHED_AS_NULL,
    );

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
        $matchedSetOrder,
        $setOrder,
        $matchedNamed,
        $named,
        $replaced,
        $count,
        $filtered,
        $filteredCount,
        preg_split(
            '/(,+)/',
            'one,two,,three',
            -1,
            PREG_SPLIT_DELIM_CAPTURE | PREG_SPLIT_OFFSET_CAPTURE,
        ),
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

<?php
// runtime-semantics: category=strings expect=pass

function native_pcre_wrap(array $matches): string
{
    return '<' . $matches[0] . '>';
}

function native_pcre_bracket(array $matches): string
{
    return '[' . $matches[0] . ']';
}

function native_pcre_length(array $matches): int
{
    return strlen($matches[0]);
}

function run_native_pcre_callback(string $pattern, string $subject): array
{
    $count = -1;
    $result = preg_replace_callback(
        $pattern,
        'native_pcre_wrap',
        $subject,
        2,
        $count,
    );

    return [$result, $count];
}

function run_runtime_native_pcre_callback(
    callable $callback,
    string $pattern,
    string $subject,
): array {
    $count = -1;
    $result = preg_replace_callback(
        $pattern,
        $callback,
        $subject,
        2,
        $count,
    );

    return [$result, $count];
}

function run_native_pcre_callback_array(string $subject): array
{
    $count = -1;
    $result = preg_replace_callback_array(
        [
            '/[a-z]+/' => 'native_pcre_wrap',
            '/[0-9]+/' => 'native_pcre_length',
        ],
        $subject,
        2,
        $count,
    );

    return [$result, $count];
}

function run_runtime_native_pcre_callback_array(
    callable $letters,
    callable $digits,
    string $subject,
): array {
    $count = -1;
    $result = preg_replace_callback_array(
        [
            '/[a-z]+/' => $letters,
            '/[0-9]+/' => $digits,
        ],
        $subject,
        2,
        $count,
    );

    return [$result, $count];
}

function run_native_pcre_callback_array_failure(): array
{
    $count = -1;
    $result = preg_replace_callback_array(
        [
            '/./u' => 'native_pcre_wrap',
        ],
        "\xFF",
        -1,
        $count,
    );

    return [
        $result,
        $count,
        preg_last_error(),
        preg_last_error_msg(),
    ];
}

function run_native_pcre_closure_callback_array(string $subject): array
{
    $prefix = '!';
    $count = -1;
    $result = preg_replace_callback_array(
        [
            '/[a-z]+/' => static function (array $matches) use ($prefix): string {
                return $prefix . $matches[0];
            },
        ],
        $subject,
        2,
        $count,
    );

    return [$result, $count];
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = [
        run_native_pcre_callback('/[a-z]+/', 'one 22 two 333 three'),
        run_runtime_native_pcre_callback(
            'native_pcre_length',
            '/[a-z]+/',
            'one 22 two 333 three',
        ),
        run_native_pcre_callback_array('one 22 two 333 three'),
        run_runtime_native_pcre_callback_array(
            'native_pcre_wrap',
            'native_pcre_length',
            'one 22 two 333 three',
        ),
        run_native_pcre_callback_array_failure(),
        run_native_pcre_closure_callback_array('one 22 two'),
    ];
}

var_dump($result);

<?php

function native_boolean_comparisons(): array
{
    $truth = true;
    $falsehood = false;
    $nothing = null;
    $zero = 0;
    $one = 1;
    $empty = "";
    $text = "text";

    return [
        $truth == $one,
        $falsehood == $zero,
        $nothing == $empty,
        $nothing != $text,
        $truth > $falsehood,
        $nothing < $truth,
        $falsehood <= $nothing,
        $truth >= $text,
        $nothing <=> $truth,
        $truth <=> $text,
    ];
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = native_boolean_comparisons();
}
var_dump($result);

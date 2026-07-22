<?php

function native_numeric_comparisons(): array
{
    $fraction = 3.5;
    $integer = 3;
    $sameFloat = 3.0;
    $sameInteger = 3;
    $infinity = 1.0e308 * 1.0e308;
    $nan = $infinity - $infinity;

    return [
        $fraction > $integer,
        $fraction >= $integer,
        $fraction < $integer,
        $fraction <= $integer,
        $fraction == $integer,
        $fraction != $integer,
        $sameFloat == $sameInteger,
        $sameFloat === $sameInteger,
        $sameFloat !== $sameInteger,
        $sameFloat === 3.0,
        $integer <=> $fraction,
        $fraction <=> $integer,
        $sameFloat <=> $sameInteger,
        $nan == $nan,
        $nan != $nan,
        $nan < $integer,
        $nan <= $integer,
        $nan > $integer,
        $nan >= $integer,
        $nan <=> $integer,
    ];
}

$result = null;
for ($iteration = 0; $iteration < 32; $iteration++) {
    $result = native_numeric_comparisons();
}
var_dump($result);

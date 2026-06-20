<?php

$value = 0;

if ($value === 0) {
    echo "zero";
} elseif ($value === 1) {
    echo "one";
} else {
    echo "many";
}

while ($value < 3) {
    $value = $value + 1;
}

do {
    $value = $value - 1;
} while ($value > 0);

for ($i = 0; $i < 3; $i = $i + 1) {
    if ($i === 1) {
        continue;
    }
}

foreach ([1 => 2, 3 => 4] as $key => &$item) {
    $item = $item + $key;
    if ($item > 5) {
        break 1;
    }
}

switch ($value) {
    case 0:
        echo "zero";
        break;
    case 1:
        echo "one";
        break;
    default:
        throw new RuntimeException("unexpected");
}

function control_return(int $input): int {
    if ($input < 0) {
        return 0;
    }

    return $input;
}

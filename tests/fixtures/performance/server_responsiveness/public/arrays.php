<?php
$values = range(1, 80);
$sum = 0;
foreach ($values as $value) {
    $sum += $value;
}
echo "arrays:", $sum, "\n";

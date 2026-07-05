<?php

$rows = [];
for ($i = 0; $i < 25; $i++) {
    $rows[] = 'fixed';
    $rows['k' . $i] = 42;
}
echo count($rows), ':', $rows['k3'], ':', $rows[0], "\n";

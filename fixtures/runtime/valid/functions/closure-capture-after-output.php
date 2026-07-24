<?php
$x = 2;
$f = function () use ($x) {
    return $x;
};
$x = 100;

echo $f(), "\n";
echo $f(), "\n";

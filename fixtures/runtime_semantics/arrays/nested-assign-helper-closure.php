<?php

$array = array();
$outer = 'alpha';
$inner = 'beta';
$array[$outer] = array();
$array[$outer][$inner] = 42;
var_dump($array);

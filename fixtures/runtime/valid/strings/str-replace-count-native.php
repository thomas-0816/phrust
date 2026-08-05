<?php
$count = -1;
var_dump(str_replace('a', 'xy', 'banana', $count));
var_dump($count);

$count = -1;
var_dump(str_replace('', 'x', 'plain', $count));
var_dump($count);

$count = -1;
var_dump(str_replace('z', 'x', 'plain', $count));
var_dump($count);

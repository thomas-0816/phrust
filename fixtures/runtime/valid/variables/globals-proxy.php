<?php

$alpha = 1;
echo $GLOBALS['alpha'], '|';

$GLOBALS['alpha'] = 2;
echo $alpha, '|';

$alias =& $GLOBALS['alpha'];
$alias = 3;
echo $GLOBALS['alpha'], '|';

unset($GLOBALS['alpha']);
echo isset($alpha) ? 'present' : 'missing', '|';

$GLOBALS['nested'] = ['x' => 1];
$GLOBALS['nested']['x'] = 2;
echo $nested['x'], '|';
echo array_key_exists('nested', $GLOBALS) ? 'present' : 'missing', "\n";

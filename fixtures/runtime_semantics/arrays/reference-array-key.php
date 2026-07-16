<?php
$name = 'variation';
$style = array( 'name' => &$name );
$selectors = array();

$selectors[ $style['name'] ] = '.is-style-' . $style['name'];

echo $selectors['variation'], "\n";

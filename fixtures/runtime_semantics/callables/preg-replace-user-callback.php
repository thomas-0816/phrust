<?php

function bracket_word(array $matches): string
{
    return '[' . strtoupper($matches[1]) . ']';
}

$count = 0;
var_dump(preg_replace_callback('/([a-z]+)/', 'bracket_word', 'one two', -1, $count));
var_dump($count);
var_dump(preg_replace_callback('/([a-z]+)/', 'bracket_word', array('x' => 'three')));

<?php
// invalid: function and arrow function declarations are malformed

function broken(string $name {
    echo $name;
}

$badArrow = fn($x) $x;

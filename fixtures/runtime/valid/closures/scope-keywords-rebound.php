<?php
// Reference-confirmed (PHP 8.5.7): binding an unscoped closure to an object
// supplies the class scope, and static:: reports the PHP-visible spelling.
$fn = function () { return static::class; };
class MixedCase {}
$bound = \Closure::bind($fn, new MixedCase());
echo $bound();

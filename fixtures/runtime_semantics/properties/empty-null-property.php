<?php
// runtime-semantics: category=properties expect=pass

$value = null;
$property = 'missing';

var_dump(isset($value->missing));
var_dump(isset($value->{$property}));
var_dump(isset($value->missing['nested']));
var_dump(empty($value->missing));
var_dump(empty($value->{$property}));
var_dump(empty($value->missing['nested']));

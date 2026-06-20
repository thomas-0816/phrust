<?php

$name = "Ada";
$arr = ["key" => "value"];
$obj = new class {
    public string $prop = "property";
};

$text = "Hello $name $arr[key] $obj->prop {$arr['key']} ${name}";
$shell = `printf $name`;

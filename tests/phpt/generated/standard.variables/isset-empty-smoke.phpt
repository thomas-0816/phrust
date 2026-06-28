--TEST--
standard.variables: isset and empty smoke
--DESCRIPTION--
Generated focused coverage for isset() and empty() language constructs.
--FILE--
<?php
$values = [
    "unset" => null,
    "zero_int" => 0,
    "zero_string" => "0",
    "false" => false,
    "empty_string" => "",
    "empty_array" => [],
    "one" => 1,
    "text" => "php",
    "array" => [0],
];
unset($values["unset"]);

foreach (["unset", "zero_int", "zero_string", "false", "empty_string", "empty_array", "one", "text", "array"] as $key) {
    echo $key, ":";
    var_dump(isset($values[$key]));
    echo $key, ":";
    var_dump(empty($values[$key]));
}
?>
--EXPECT--
unset:bool(false)
unset:bool(true)
zero_int:bool(true)
zero_int:bool(true)
zero_string:bool(true)
zero_string:bool(true)
false:bool(true)
false:bool(true)
empty_string:bool(true)
empty_string:bool(true)
empty_array:bool(true)
empty_array:bool(true)
one:bool(true)
one:bool(false)
text:bool(true)
text:bool(false)
array:bool(true)
array:bool(false)

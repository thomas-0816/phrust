--TEST--
json: encode common flags
--DESCRIPTION--
Generated focused Prompt 17.1 coverage for JSON_PRETTY_PRINT, JSON_UNESCAPED_SLASHES, JSON_UNESCAPED_UNICODE, and JSON_PRESERVE_ZERO_FRACTION.
--FILE--
<?php
var_dump(json_encode(
    array("url" => "https://example.test/a b", "snow" => "☃", "n" => 1.0),
    JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE | JSON_PRESERVE_ZERO_FRACTION
));
echo json_encode(array("a" => array(1, 2)), JSON_PRETTY_PRINT), "\n";
?>
--EXPECT--
string(55) "{"url":"https://example.test/a b","snow":"☃","n":1.0}"
{
    "a": [
        1,
        2
    ]
}

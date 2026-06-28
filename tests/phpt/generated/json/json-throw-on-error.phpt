--TEST--
json: JSON_THROW_ON_ERROR decode failure
--DESCRIPTION--
Generated focused Prompt 17.1 coverage for the JSON_THROW_ON_ERROR failure surface.
--FILE--
<?php
try {
    json_decode('{', false, 512, JSON_THROW_ON_ERROR);
} catch (JsonException $e) {
    echo get_class($e), ': ', $e->getMessage(), "\n";
}
?>
--EXPECT--
JsonException: Syntax error

<?php
// runtime-semantics: expect=pass
$s = "abc";
$s[-5] = "Q";
echo $s, "\n";

try {
    $s["name"] = "Q";
} catch (Throwable $error) {
    echo get_class($error), ":", $error->getMessage(), "\n";
}

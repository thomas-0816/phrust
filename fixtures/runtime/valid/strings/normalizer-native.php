<?php
var_dump(normalizer_is_normalized('native'));
var_dump(normalizer_normalize('native'));

try {
    normalizer_is_normalized('native', 999);
} catch (ValueError $error) {
    echo get_class($error), ':', $error->getMessage(), "\n";
}

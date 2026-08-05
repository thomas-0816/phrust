<?php

function native_prefixed_id($prefix) {
    return uniqid($prefix . "-");
}

$plain = uniqid();
$prefixed = native_prefixed_id("pre");
$entropy = uniqid("e-", true);

echo strlen($plain), "|";
echo substr($prefixed, 0, 4), "|", strlen($prefixed), "|";
echo substr($entropy, 0, 2), "|", strlen($entropy), "\n";

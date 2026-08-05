<?php
$hash = sodium_crypto_generichash("native", "wp_fast_hash_6.8+", 30);
echo strlen($hash), "\n";
echo bin2hex($hash), "\n";
echo sodium_bin2base64($hash, SODIUM_BASE64_VARIANT_URLSAFE_NO_PADDING), "\n";

<?php

echo base_convert("ff", 16, 2), "\n";
echo bindec("101010"), ":", hexdec("ff"), ":", octdec("377"), "\n";
echo decbin(42), ":", dechex(255), ":", decoct(64), "\n";
echo base_convert("zz", 36, 10), "\n";
echo decbin(12.0), "\n";

try {
    base_convert("10", 1, 10);
} catch (ValueError) {
    echo "invalid-base\n";
}

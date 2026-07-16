--TEST--
PHPT generated regression: Test header_register_callback
--DESCRIPTION--
original php-src path: tests/basic/header_register_callback.phpt
original source hash: 1f481066a21647877b48819868d7263d35d3edf45445ffefca8b3b1abe9358fc
generated timestamp: 20260715T154100Z
generator version: phpt-generate-v1
reason: known target failure minimized against reference output
--FILE--
<?php
header_register_callback(function() { echo "sent";});
--EXPECT--
sent

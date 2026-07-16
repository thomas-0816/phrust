--TEST--
PHPT generated regression: output buffering - ob_clean
--DESCRIPTION--
original php-src path: tests/output/ob_004.phpt
original source hash: 382b832e857d590a9f9b2b495005ea59dfb5c92ad807be047fba012d4904ce38
generated timestamp: 20260715T154100Z
generator version: phpt-generate-v1
reason: known target failure minimized against reference output
--FILE--
<?php
ob_start();
echo "bar\n";
--EXPECT--
bar

--TEST--
Generated smoke: missing include emits PHP warnings and continues
--DESCRIPTION--
reference behavior: PHP 8.5.7 CLI missing include diagnostics
generated timestamp: 20260628T000000Z
generator version: phpt-diagnostics-output-v1
reason: central include warning output formatting and continuation
--FILE--
<?php
include "diagnostics-output-missing.php";
echo "after\n";
--EXPECTF--
Warning: include(diagnostics-output-missing.php): Failed to open stream: No such file or directory in %s on line %d

Warning: include(): Failed opening 'diagnostics-output-missing.php' for inclusion (include_path='%s') in %s on line %d
after

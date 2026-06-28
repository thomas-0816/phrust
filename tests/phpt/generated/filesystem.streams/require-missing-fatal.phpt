--TEST--
filesystem.streams: require missing fatal
--DESCRIPTION--
Generated include baseline covering missing local require as a fatal error.
--INI--
display_errors=1
--CAPTURE_STDIO--
STDOUT
--FILE--
<?php
echo "before|";
require "require-missing-fatal-child.php";
echo "after";
?>
--EXPECTREGEX--
before\|
Warning: require\([^\r\n]*require-missing-fatal-child\.php\): Failed to open stream: No such file or directory in [^\r\n]*require-missing-fatal\.php on line \d+

Fatal error: Uncaught Error: Failed opening required '[^\r\n]*require-missing-fatal-child\.php' \(include_path='[^']*'\) in [^\r\n]*require-missing-fatal\.php:\d+
Stack trace:
#0 \{main\}
  thrown in [^\r\n]*require-missing-fatal\.php on line \d+

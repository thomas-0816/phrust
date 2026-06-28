--TEST--
standard.output: clean discards and flush forwards active buffers
--FILE--
<?php
ob_start();
echo "discard";
$cleaned = ob_end_clean();
echo "after-clean|";
ob_start();
echo "flush";
$flushed = ob_end_flush();
echo "|after-flush";
var_dump($cleaned, $flushed);
?>
--EXPECT--
after-clean|flush|after-flushbool(true)
bool(true)

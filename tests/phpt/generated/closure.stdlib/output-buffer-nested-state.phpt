--TEST--
closure.stdlib: nested output buffer state and flush paths
--DESCRIPTION--
Generated closure stdlib coverage for nested output buffers, read length,
clean, flush, and level state.
--FILE--
<?php
var_dump(ob_get_level());
ob_start();
echo "outer:";
ob_start();
echo "inner";
$state = [ob_get_level(), ob_get_contents(), ob_get_length()];
$inner = ob_get_clean();
echo "[" . $inner . "]";
$ended = ob_end_flush();
flush();
var_dump($ended);
var_dump($state);
var_dump(ob_get_level());
?>
--EXPECT--
int(0)
outer:[inner]bool(true)
array(3) {
  [0]=>
  int(2)
  [1]=>
  string(5) "inner"
  [2]=>
  int(5)
}
int(0)

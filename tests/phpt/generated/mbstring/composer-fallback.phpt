--TEST--
mbstring: Composer-style fallback branch
--DESCRIPTION--
Focused mbstring stub coverage for code that branches on extension_loaded("mbstring").
--FILE--
<?php
if (extension_loaded("mbstring")) {
    echo "unexpected mbstring success\n";
} else {
    echo "mbstring platform requirement is unsatisfied\n";
}
?>
--EXPECT--
mbstring platform requirement is unsatisfied

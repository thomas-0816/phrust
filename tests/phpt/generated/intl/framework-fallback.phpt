--TEST--
intl: framework-style fallback branch
--DESCRIPTION--
Focused intl stub coverage for code that branches on extension_loaded("intl").
--FILE--
<?php
if (extension_loaded("intl")) {
    echo "unexpected intl success\n";
} else {
    echo "intl platform requirement is unsatisfied\n";
}
?>
--EXPECT--
intl platform requirement is unsatisfied

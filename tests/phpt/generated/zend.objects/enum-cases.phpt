--TEST--
Generated zend.objects: enum cases
--DESCRIPTION--
module: zend.objects
generated timestamp: 20260627T000000Z
generator version: phpt-objects-traits-enums-v1
reason: enum cases baseline
--FILE--
<?php
enum ObjectCasesStatus {
    case Draft;
    case Ready;
}

foreach (ObjectCasesStatus::cases() as $case) {
    echo $case->name, "|";
}
echo "\n";
?>
--EXPECT--
Draft|Ready|

--TEST--
Generated objects.enums: unit enum case
--DESCRIPTION--
module: objects.enums
generated timestamp: 20260627T000000Z
generator version: phpt-objects-traits-enums-v1
reason: unit enum case baseline
--FILE--
<?php
enum ObjectUnitStatus {
    case Draft;
    case Published;
}

echo ObjectUnitStatus::Draft->name, "|";
if (ObjectUnitStatus::Draft === ObjectUnitStatus::Draft) {
    echo "same\n";
} else {
    echo "different\n";
}
?>
--EXPECT--
Draft|same

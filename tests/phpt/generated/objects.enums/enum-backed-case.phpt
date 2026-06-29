--TEST--
Generated objects.enums: backed enum case
--DESCRIPTION--
module: objects.enums
generated timestamp: 20260627T000000Z
generator version: phpt-objects-traits-enums-v1
reason: backed enum case baseline
--FILE--
<?php
enum ObjectBackedStatus: string {
    case Ready = "ready";
    case Done = "done";
}

echo ObjectBackedStatus::Ready->name, "|", ObjectBackedStatus::Ready->value, "\n";
?>
--EXPECT--
Ready|ready

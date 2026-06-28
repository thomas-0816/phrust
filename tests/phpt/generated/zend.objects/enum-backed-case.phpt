--TEST--
Generated zend.objects: backed enum case
--DESCRIPTION--
module: zend.objects
generated timestamp: 20260627T000000Z
generator version: phpt-objects-traits-enums-v1
reason: Prompt 14.9 backed enum case baseline
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

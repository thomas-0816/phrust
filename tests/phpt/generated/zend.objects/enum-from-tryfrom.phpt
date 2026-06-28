--TEST--
Generated zend.objects: backed enum from and tryFrom
--DESCRIPTION--
module: zend.objects
generated timestamp: 20260627T000000Z
generator version: phpt-objects-traits-enums-v1
reason: Prompt 14.9 backed enum from/tryFrom baseline
--FILE--
<?php
enum ObjectLookupStatus: string {
    case Ready = "ready";
    case Done = "done";
}

$ready = ObjectLookupStatus::from("ready");
echo $ready->name, "|", $ready->value, "|";
$missing = ObjectLookupStatus::tryFrom("missing");
if ($missing === null) {
    echo "null\n";
} else {
    echo "object\n";
}
?>
--EXPECT--
Ready|ready|null

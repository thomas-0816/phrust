<?php
// oracle-probe: id=oracle-reflection-parameter-array-multisort-351d892d0c area=reflection kind=parameter symbol=array_multisort source=php-src expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-reflection-parameter-array-multisort-351d892d0c failure_category=reflection
$ref = new ReflectionFunction("array_multisort");
$found = "none";
foreach ($ref->getParameters() as $param) {
    if ($param->isPassedByReference()) { $found = $param->getName(); break; }
}
echo $ref->getName(), ":", $found, "\n";

<?php
// oracle-probe: id=oracle-reflection-parameter-array-pop-2958741fc4 area=reflection kind=parameter symbol=array_pop source=php-src expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-reflection-parameter-array-pop-2958741fc4 failure_category=reflection
$ref = new ReflectionFunction("array_pop");
$found = "none";
foreach ($ref->getParameters() as $param) {
    if ($param->isPassedByReference()) { $found = $param->getName(); break; }
}
echo $ref->getName(), ":", $found, "\n";

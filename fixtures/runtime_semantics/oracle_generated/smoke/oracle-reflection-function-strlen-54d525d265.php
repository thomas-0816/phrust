<?php
// oracle-probe: id=oracle-reflection-function-strlen-54d525d265 area=reflection kind=function symbol=strlen source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-reflection-function-strlen-54d525d265 failure_category=reflection
$ref = new ReflectionFunction("strlen");
echo $ref->getName(), ":", $ref->getNumberOfParameters(), "\n";
$param = $ref->getParameters()[0];
echo $param->getName(), ":", $param->isOptional() ? "optional" : "required", "\n";

<?php
// oracle-probe: id=oracle-builtin-contract-function-property-exists-d9a704b447 area=builtin_contract kind=function symbol=property_exists source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-property-exists-d9a704b447 failure_category=builtin_contract
$name = "property_exists";
echo function_exists($name) ? "available\n" : "missing\n";

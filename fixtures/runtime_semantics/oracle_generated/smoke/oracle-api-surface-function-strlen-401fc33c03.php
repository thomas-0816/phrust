<?php
// oracle-probe: id=oracle-api-surface-function-strlen-401fc33c03 area=api_surface kind=function symbol=strlen source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-api-surface-function-strlen-401fc33c03 failure_category=api_surface
echo function_exists("strlen") ? "function\n" : "missing\n";

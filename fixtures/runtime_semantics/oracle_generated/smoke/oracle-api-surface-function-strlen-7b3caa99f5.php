<?php
// oracle-probe: id=oracle-api-surface-function-strlen-7b3caa99f5 area=api_surface kind=function symbol=strlen source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-api-surface-function-strlen-7b3caa99f5 failure_category=api_surface
echo function_exists("strlen") ? "function\n" : "missing\n";

<?php
// oracle-probe: id=oracle-builtin-contract-function-phpinfo-d30ba546ed area=builtin_contract kind=function symbol=phpinfo source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-phpinfo-d30ba546ed failure_category=builtin_contract
$name = "phpinfo";
echo function_exists($name) ? "available\n" : "missing\n";

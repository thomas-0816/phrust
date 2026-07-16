<?php
// oracle-probe: id=oracle-builtin-contract-function-spl-autoload-call-0fd9dcdad6 area=builtin_contract kind=function symbol=spl_autoload_call source=ext/spl/php_spl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-spl-autoload-call-0fd9dcdad6 failure_category=builtin_contract requires_ref_extension=spl
$name = "spl_autoload_call";
echo function_exists($name) ? "available\n" : "missing\n";

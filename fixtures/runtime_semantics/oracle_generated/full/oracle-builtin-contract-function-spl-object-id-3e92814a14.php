<?php
// oracle-probe: id=oracle-builtin-contract-function-spl-object-id-3e92814a14 area=builtin_contract kind=function symbol=spl_object_id source=ext/spl/php_spl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-spl-object-id-3e92814a14 failure_category=builtin_contract requires_ref_extension=spl
$name = "spl_object_id";
echo function_exists($name) ? "available\n" : "missing\n";

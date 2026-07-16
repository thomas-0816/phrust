<?php
// oracle-probe: id=oracle-builtin-contract-function-finfo-set-flags-e201df94a1 area=builtin_contract kind=function symbol=finfo_set_flags source=ext/fileinfo/fileinfo.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-finfo-set-flags-e201df94a1 failure_category=builtin_contract requires_ref_extension=fileinfo
$name = "finfo_set_flags";
echo function_exists($name) ? "available\n" : "missing\n";

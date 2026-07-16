<?php
// oracle-probe: id=oracle-builtin-contract-function-finfo-file-e576a1dc5a area=builtin_contract kind=function symbol=finfo_file source=ext/fileinfo/fileinfo.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-finfo-file-e576a1dc5a failure_category=builtin_contract requires_ref_extension=fileinfo
$name = "finfo_file";
echo function_exists($name) ? "available\n" : "missing\n";

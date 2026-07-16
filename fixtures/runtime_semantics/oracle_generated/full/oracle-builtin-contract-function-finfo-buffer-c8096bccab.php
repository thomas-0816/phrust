<?php
// oracle-probe: id=oracle-builtin-contract-function-finfo-buffer-c8096bccab area=builtin_contract kind=function symbol=finfo_buffer source=ext/fileinfo/fileinfo.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-finfo-buffer-c8096bccab failure_category=builtin_contract requires_ref_extension=fileinfo
$name = "finfo_buffer";
echo function_exists($name) ? "available\n" : "missing\n";

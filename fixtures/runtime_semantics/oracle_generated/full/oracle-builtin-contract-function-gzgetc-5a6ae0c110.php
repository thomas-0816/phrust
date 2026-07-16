<?php
// oracle-probe: id=oracle-builtin-contract-function-gzgetc-5a6ae0c110 area=builtin_contract kind=function symbol=gzgetc source=ext/zlib/zlib.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-gzgetc-5a6ae0c110 failure_category=builtin_contract requires_ref_extension=zlib
$name = "gzgetc";
echo function_exists($name) ? "available\n" : "missing\n";

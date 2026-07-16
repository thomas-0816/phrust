<?php
// oracle-probe: id=oracle-builtin-contract-function-gzuncompress-d91aa39eec area=builtin_contract kind=function symbol=gzuncompress source=ext/zlib/zlib.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-gzuncompress-d91aa39eec failure_category=builtin_contract requires_ref_extension=zlib
$name = "gzuncompress";
echo function_exists($name) ? "available\n" : "missing\n";

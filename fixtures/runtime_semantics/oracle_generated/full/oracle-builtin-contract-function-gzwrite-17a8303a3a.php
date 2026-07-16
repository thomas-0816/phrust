<?php
// oracle-probe: id=oracle-builtin-contract-function-gzwrite-17a8303a3a area=builtin_contract kind=function symbol=gzwrite source=ext/zlib/zlib.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-gzwrite-17a8303a3a failure_category=builtin_contract requires_ref_extension=zlib
$name = "gzwrite";
echo function_exists($name) ? "available\n" : "missing\n";

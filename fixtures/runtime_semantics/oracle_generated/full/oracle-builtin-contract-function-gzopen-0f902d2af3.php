<?php
// oracle-probe: id=oracle-builtin-contract-function-gzopen-0f902d2af3 area=builtin_contract kind=function symbol=gzopen source=ext/zlib/zlib.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-gzopen-0f902d2af3 failure_category=builtin_contract requires_ref_extension=zlib
$name = "gzopen";
echo function_exists($name) ? "available\n" : "missing\n";

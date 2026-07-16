<?php
// oracle-probe: id=oracle-builtin-contract-function-gzgets-22d8a4a839 area=builtin_contract kind=function symbol=gzgets source=ext/zlib/zlib.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-gzgets-22d8a4a839 failure_category=builtin_contract requires_ref_extension=zlib
$name = "gzgets";
echo function_exists($name) ? "available\n" : "missing\n";

<?php
// oracle-probe: id=oracle-builtin-contract-function-gzrewind-1638bf9e4d area=builtin_contract kind=function symbol=gzrewind source=ext/zlib/zlib.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-gzrewind-1638bf9e4d failure_category=builtin_contract requires_ref_extension=zlib
$name = "gzrewind";
echo function_exists($name) ? "available\n" : "missing\n";

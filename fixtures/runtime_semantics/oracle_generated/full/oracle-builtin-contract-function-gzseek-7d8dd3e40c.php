<?php
// oracle-probe: id=oracle-builtin-contract-function-gzseek-7d8dd3e40c area=builtin_contract kind=function symbol=gzseek source=ext/zlib/zlib.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-gzseek-7d8dd3e40c failure_category=builtin_contract requires_ref_extension=zlib
$name = "gzseek";
echo function_exists($name) ? "available\n" : "missing\n";

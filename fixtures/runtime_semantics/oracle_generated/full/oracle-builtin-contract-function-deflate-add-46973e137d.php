<?php
// oracle-probe: id=oracle-builtin-contract-function-deflate-add-46973e137d area=builtin_contract kind=function symbol=deflate_add source=ext/zlib/zlib.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-deflate-add-46973e137d failure_category=builtin_contract requires_ref_extension=zlib
$name = "deflate_add";
echo function_exists($name) ? "available\n" : "missing\n";

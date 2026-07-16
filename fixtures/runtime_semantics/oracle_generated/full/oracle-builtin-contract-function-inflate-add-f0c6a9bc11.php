<?php
// oracle-probe: id=oracle-builtin-contract-function-inflate-add-f0c6a9bc11 area=builtin_contract kind=function symbol=inflate_add source=ext/zlib/zlib.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-inflate-add-f0c6a9bc11 failure_category=builtin_contract requires_ref_extension=zlib
$name = "inflate_add";
echo function_exists($name) ? "available\n" : "missing\n";

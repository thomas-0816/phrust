<?php
// oracle-probe: id=oracle-builtin-contract-function-mb-encoding-aliases-8ebf7107ed area=builtin_contract kind=function symbol=mb_encoding_aliases source=ext/mbstring/mbstring.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-mb-encoding-aliases-8ebf7107ed failure_category=builtin_contract requires_ref_extension=mbstring
$name = "mb_encoding_aliases";
echo function_exists($name) ? "available\n" : "missing\n";

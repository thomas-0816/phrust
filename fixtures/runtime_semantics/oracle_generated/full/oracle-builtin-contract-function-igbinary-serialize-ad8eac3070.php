<?php
// oracle-probe: id=oracle-builtin-contract-function-igbinary-serialize-ad8eac3070 area=builtin_contract kind=function symbol=igbinary_serialize source=Rust extension registry expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-igbinary-serialize-ad8eac3070 failure_category=builtin_contract requires_ref_extension=igbinary
$name = "igbinary_serialize";
echo function_exists($name) ? "available\n" : "missing\n";

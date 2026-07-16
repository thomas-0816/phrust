<?php
// oracle-probe: id=oracle-builtin-contract-function-imagecreatefrompng-d626a519c7 area=builtin_contract kind=function symbol=imagecreatefrompng source=ext/gd/gd.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-imagecreatefrompng-d626a519c7 failure_category=builtin_contract requires_ref_extension=gd
$name = "imagecreatefrompng";
echo function_exists($name) ? "available\n" : "missing\n";

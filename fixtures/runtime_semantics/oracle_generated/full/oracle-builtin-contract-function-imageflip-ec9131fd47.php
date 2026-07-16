<?php
// oracle-probe: id=oracle-builtin-contract-function-imageflip-ec9131fd47 area=builtin_contract kind=function symbol=imageflip source=ext/gd/gd.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-imageflip-ec9131fd47 failure_category=builtin_contract requires_ref_extension=gd
$name = "imageflip";
echo function_exists($name) ? "available\n" : "missing\n";

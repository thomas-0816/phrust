<?php
// oracle-probe: id=oracle-builtin-contract-function-image-type-to-extension-061c428a21 area=builtin_contract kind=function symbol=image_type_to_extension source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-image-type-to-extension-061c428a21 failure_category=builtin_contract
$name = "image_type_to_extension";
echo function_exists($name) ? "available\n" : "missing\n";

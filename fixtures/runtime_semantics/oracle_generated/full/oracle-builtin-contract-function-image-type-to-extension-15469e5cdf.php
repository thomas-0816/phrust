<?php
// oracle-probe: id=oracle-builtin-contract-function-image-type-to-extension-15469e5cdf area=builtin_contract kind=function symbol=image_type_to_extension source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-image-type-to-extension-15469e5cdf failure_category=builtin_contract
try {
    $result = \image_type_to_extension();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}

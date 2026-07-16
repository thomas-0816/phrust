<?php
// oracle-probe: id=oracle-builtin-contract-function-image-type-to-mime-type-40e2f616bd area=builtin_contract kind=function symbol=image_type_to_mime_type source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-image-type-to-mime-type-40e2f616bd failure_category=builtin_contract
try {
    $result = \image_type_to_mime_type();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}

<?php
// oracle-probe: id=oracle-builtin-contract-function-imagefilledrectangle-9be39ae63b area=builtin_contract kind=function symbol=imagefilledrectangle source=ext/gd/gd.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-imagefilledrectangle-9be39ae63b failure_category=builtin_contract requires_ref_extension=gd
try {
    $result = \imagefilledrectangle();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}

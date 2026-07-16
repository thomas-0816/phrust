<?php
// oracle-probe: id=oracle-builtin-contract-function-ctype-alnum-36ea0bc05f area=builtin_contract kind=function symbol=ctype_alnum source=ext/ctype/ctype.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-ctype-alnum-36ea0bc05f failure_category=builtin_contract requires_ref_extension=ctype
try {
    $result = \ctype_alnum();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}

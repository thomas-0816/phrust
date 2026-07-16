<?php
// oracle-probe: id=oracle-builtin-behavior-function-ctype-lower-5c5b5a8149 area=builtin_behavior kind=function symbol=ctype_lower source=ext/ctype/ctype.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-ctype-lower-5c5b5a8149 failure_category=builtin_behavior requires_ref_extension=ctype
try {
    $result = \ctype_lower(text: null);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}

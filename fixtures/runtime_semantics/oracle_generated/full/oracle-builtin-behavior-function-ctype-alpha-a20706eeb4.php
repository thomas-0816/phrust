<?php
// oracle-probe: id=oracle-builtin-behavior-function-ctype-alpha-a20706eeb4 area=builtin_behavior kind=function symbol=ctype_alpha source=ext/ctype/ctype.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-ctype-alpha-a20706eeb4 failure_category=builtin_behavior requires_ref_extension=ctype
try {
    $result = \ctype_alpha(null);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}

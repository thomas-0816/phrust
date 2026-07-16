<?php
// oracle-probe: id=oracle-builtin-behavior-function-ctype-xdigit-9c5fbb2f30 area=builtin_behavior kind=function symbol=ctype_xdigit source=ext/ctype/ctype.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-ctype-xdigit-9c5fbb2f30 failure_category=builtin_behavior requires_ref_extension=ctype
try {
    $result = \ctype_xdigit(null);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}

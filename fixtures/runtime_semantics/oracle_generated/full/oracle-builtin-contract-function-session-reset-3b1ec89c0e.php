<?php
// oracle-probe: id=oracle-builtin-contract-function-session-reset-3b1ec89c0e area=builtin_contract kind=function symbol=session_reset source=ext/session/session.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-session-reset-3b1ec89c0e failure_category=builtin_contract requires_ref_extension=session
try {
    $result = \session_reset(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}

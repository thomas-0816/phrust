<?php
// oracle-probe: id=oracle-builtin-contract-function-session-decode-215d5d9dd7 area=builtin_contract kind=function symbol=session_decode source=ext/session/session.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-session-decode-215d5d9dd7 failure_category=builtin_contract requires_ref_extension=session
try {
    $result = \session_decode();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}

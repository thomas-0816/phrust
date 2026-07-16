<?php
// oracle-probe: id=oracle-builtin-contract-function-ftp-systype-10e208d969 area=builtin_contract kind=function symbol=ftp_systype source=ext/ftp/ftp.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-ftp-systype-10e208d969 failure_category=builtin_contract requires_ref_extension=ftp
try {
    $result = \ftp_systype();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}

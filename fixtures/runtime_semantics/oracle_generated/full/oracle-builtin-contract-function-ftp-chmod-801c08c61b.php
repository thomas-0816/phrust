<?php
// oracle-probe: id=oracle-builtin-contract-function-ftp-chmod-801c08c61b area=builtin_contract kind=function symbol=ftp_chmod source=ext/ftp/ftp.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-ftp-chmod-801c08c61b failure_category=builtin_contract requires_ref_extension=ftp
$name = "ftp_chmod";
echo function_exists($name) ? "available\n" : "missing\n";

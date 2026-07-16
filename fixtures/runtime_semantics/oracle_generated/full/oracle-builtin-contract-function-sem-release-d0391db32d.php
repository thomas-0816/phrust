<?php
// oracle-probe: id=oracle-builtin-contract-function-sem-release-d0391db32d area=builtin_contract kind=function symbol=sem_release source=ext/sysvsem/sysvsem.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-sem-release-d0391db32d failure_category=builtin_contract requires_ref_extension=sysvsem
$name = "sem_release";
echo function_exists($name) ? "available\n" : "missing\n";

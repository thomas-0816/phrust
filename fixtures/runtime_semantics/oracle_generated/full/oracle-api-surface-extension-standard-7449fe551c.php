<?php
// oracle-probe: id=oracle-api-surface-extension-standard-7449fe551c area=api_surface kind=extension symbol=standard source=reference-php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-api-surface-extension-standard-7449fe551c failure_category=api_surface
echo extension_loaded("standard") ? "extension\n" : "missing\n";

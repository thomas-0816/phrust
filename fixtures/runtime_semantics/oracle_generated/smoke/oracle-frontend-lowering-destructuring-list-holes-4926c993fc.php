<?php
// oracle-probe: id=oracle-frontend-lowering-destructuring-list-holes-4926c993fc area=frontend_lowering kind=destructuring symbol=list-holes source=seed expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-frontend-lowering-destructuring-list-holes-4926c993fc failure_category=frontend_lowering
[$first, , $third] = [1, 2, 3]; echo $first, ":", $third, "\n";

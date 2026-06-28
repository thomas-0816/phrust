<?php
echo "output buffer fixture\n";
echo "level=", function_exists("ob_get_level") ? ob_get_level() : "missing", "\n";

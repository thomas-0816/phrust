<?php
echo isset($never_initialized_include_local) ? 'set' : 'unset';
if (!isset($never_initialized_include_local)) {
    echo '|entered';
}

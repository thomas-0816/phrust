<?php
namespace Fixture\Constants;

define('GLOBAL_FALLBACK_VALUE', 'global');
define('Fixture\\Constants\\LOCAL_FIRST_VALUE', 'local');
define('LOCAL_FIRST_VALUE', 'wrong-global');

echo GLOBAL_FALLBACK_VALUE, "\n";
echo LOCAL_FIRST_VALUE, "\n";

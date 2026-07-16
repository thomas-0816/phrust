<?php
function external_optional_parameter($file, $allowed_files = array())
{
    echo $file, ':', count($allowed_files), "\n";
}

function external_optional_parameter_locale()
{
    global $locale;
    if (empty($locale)) {
        $locale = 'en_US';
    }
    return $locale;
}

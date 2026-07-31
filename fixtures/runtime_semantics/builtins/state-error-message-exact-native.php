<?php
function native_state_error_messages(): array {
    json_decode("{", true);
    $json = [json_last_error(), json_last_error_msg()];

    @preg_match("/[/", "subject");
    $pcre = [preg_last_error(), preg_last_error_msg()];

    return [$json, $pcre];
}

for ($warm = 0; $warm < 32; $warm++) {
    native_state_error_messages();
}

var_dump(native_state_error_messages());

<?php
function synthetic_render_template($state)
{
    echo "template=", $state["options"]["site_name"], "\n";
    echo "plugin=", $state["options"]["plugin"], "\n";
    echo "package_size=", $state["package_size"], "\n";
    echo "upload=", $state["upload"], "\n";
    if (isset($state["upload_size"])) {
        echo "upload_size=", $state["upload_size"], "\n";
    }
}

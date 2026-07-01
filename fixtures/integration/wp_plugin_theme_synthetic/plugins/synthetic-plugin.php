<?php
function synthetic_plugin_init($state)
{
    $state["plugin_loaded"] = true;
    $state["options"]["plugin"] = "active";
    return $state;
}

function synthetic_plugin_package($state)
{
    $package_path = $state["root"] . "/var/package.txt";
    file_put_contents($package_path, "package=" . $state["options"]["plugin"]);
    $state["package_size"] = filesize($package_path);
    unlink($package_path);
    return $state;
}

function synthetic_plugin_upload($state)
{
    if (!isset($_FILES["package"])) {
        $state["upload"] = "none";
        return $state;
    }
    $target = $state["root"] . "/var/package-upload.bin";
    $state["upload"] = move_uploaded_file($_FILES["package"]["tmp_name"], $target) ? "moved" : "failed";
    if (file_exists($target)) {
        $state["upload_size"] = filesize($target);
        unlink($target);
    }
    return $state;
}

synthetic_add_action("init", "synthetic_plugin_init");
synthetic_add_action("package", "synthetic_plugin_package");
synthetic_add_action("upload", "synthetic_plugin_upload");

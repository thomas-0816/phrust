<?php
$destination = $_SERVER["DOCUMENT_ROOT"] . "/moved-upload.txt";
if (file_exists($destination)) {
    unlink($destination);
}
$tmp = $_FILES["avatar"]["tmp_name"];
echo "title=", $_POST["title"], "\n";
echo "name=", $_FILES["avatar"]["name"], "\n";
echo "type=", $_FILES["avatar"]["type"], "\n";
echo "size=", $_FILES["avatar"]["size"], "\n";
echo "error=", $_FILES["avatar"]["error"], "\n";
echo "uploaded=";
if (is_uploaded_file($tmp)) {
    echo "yes\n";
} else {
    echo "no\n";
}
$moved = move_uploaded_file($tmp, $destination);
echo "moved=";
if ($moved) {
    echo "yes\n";
    echo "content=", file_get_contents($destination), "\n";
} else {
    echo "no\n";
}
echo "uploaded_after=";
if (is_uploaded_file($tmp)) {
    echo "yes\n";
} else {
    echo "no\n";
}

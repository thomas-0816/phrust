--TEST--
curl: local multi select and info_read completion
--DESCRIPTION--
Generated coverage for libcurl-backed multi execution against a configured
loopback HTTP endpoint.
--SKIPIF--
<?php
if (!extension_loaded("curl")) {
    die("skip curl extension is not loaded");
}
if (getenv("PHRUST_NET_TESTS") !== "1") {
    die("skip PHRUST_NET_TESTS is not enabled");
}
if (getenv("PHRUST_CURL_TEST_URL") === false || getenv("PHRUST_CURL_TEST_URL") === "") {
    die("skip PHRUST_CURL_TEST_URL is not configured");
}
?>
--FILE--
<?php
$mh = curl_multi_init();
$handles = [];
for ($i = 0; $i < 2; $i++) {
    $ch = curl_init(getenv("PHRUST_CURL_TEST_URL"));
    curl_setopt($ch, CURLOPT_RETURNTRANSFER, true);
    curl_multi_add_handle($mh, $ch);
    $handles[] = $ch;
}

do {
    $status = curl_multi_exec($mh, $active);
    if ($active) {
        curl_multi_select($mh, 0.05);
    }
} while ($status === CURLM_OK && $active);

$done = 0;
$ok = 0;
while ($info = curl_multi_info_read($mh)) {
    $done++;
    if ($info["msg"] === CURLMSG_DONE && $info["result"] === 0) {
        $ok++;
    }
}

var_dump($done);
var_dump($ok);
foreach ($handles as $ch) {
    curl_multi_remove_handle($mh, $ch);
}
curl_multi_close($mh);
?>
--EXPECT--
int(2)
int(2)

--TEST--
phar: read zip and tar based archives
--DESCRIPTION--
Generated PHAR coverage for read-only phar:// reads from zip and tar archives,
plus Phar ArrayAccess reads from a PHP-generated executable zip PHAR archive.
--EXTENSIONS--
phar
--FILE--
<?php
function phar_tar_octal($value, $width) {
    return str_pad(decoct($value), $width - 1, '0', STR_PAD_LEFT) . "\0";
}

function phar_tar_entry($name, $contents) {
    $header = str_pad($name, 100, "\0");
    $header .= phar_tar_octal(420, 8);
    $header .= phar_tar_octal(0, 8);
    $header .= phar_tar_octal(0, 8);
    $header .= phar_tar_octal(strlen($contents), 12);
    $header .= phar_tar_octal(1704067200, 12);
    $header .= str_repeat(' ', 8);
    $header .= '0';
    $header .= str_repeat("\0", 100);
    $header .= "ustar\0";
    $header .= '00';
    $header .= str_repeat("\0", 247);

    $checksum = 0;
    for ($i = 0; $i < strlen($header); $i++) {
        $checksum += ord($header[$i]);
    }
    $checksumField = str_pad(decoct($checksum), 6, '0', STR_PAD_LEFT) . "\0 ";
    $header = substr($header, 0, 148) . $checksumField . substr($header, 156);
    return $header . $contents . str_repeat("\0", (512 - strlen($contents) % 512) % 512);
}

$zip = __DIR__ . '/format-fixture.phar.zip';
$tar = __DIR__ . '/format-fixture.phar.tar';
$zipHex = '504b0304000000080000c115ea5c156a2c42070000000700000008001200646174612e7478746e750e00845bb5fca40100000000000000007061796c6f6164504b0304000000080000c115ea5c9956ad723c0000003c0000000e0012002e706861722f737475622e7068706e750e00572b4184b60100000000000000003c3f706870202f2f207a69702d62617365642070686172206172636869766520737475622066696c650a5f5f48414c545f434f4d50494c455228293b504b0304000000080000000021005e6bd2142800000028000000130012002e706861722f7369676e61747572652e62696e6e750e00ff12d9410000000000000000000003000000200000002c23b960355ca3b82e4bdf74f9efdff0f69292a70105b5cf48523933030d8d0d504b01020000000000080000c115ea5c156a2c420700000007000000080012000000000000000000000000000000646174612e7478746e750e00845bb5fca4010000000000000000504b01020000000000080000c115ea5c9956ad723c0000003c0000000e001200000000000000000000003f0000002e706861722f737475622e7068706e750e00572b4184b6010000000000000000504b01020000000000080000000021005e6bd21428000000280000001300120000000000000000000000b90000002e706861722f7369676e61747572652e62696e6e750e00ff12d94100000000000000000000504b05060000000003000300e9000000240100000000';
file_put_contents($zip, hex2bin($zipHex));
file_put_contents($tar, phar_tar_entry('data.txt', 'payload') . str_repeat("\0", 1024));

$zipPhar = new Phar($zip);
var_dump(file_get_contents('phar://' . $zip . '/data.txt'));
var_dump($zipPhar['data.txt']->getContent());
var_dump($zipPhar['data.txt']->getFilename());
var_dump(file_get_contents('phar://' . $tar . '/data.txt'));
?>
--CLEAN--
<?php
@unlink(__DIR__ . '/format-fixture.phar.zip');
@unlink(__DIR__ . '/format-fixture.phar.tar');
?>
--EXPECT--
string(7) "payload"
string(7) "payload"
string(8) "data.txt"
string(7) "payload"

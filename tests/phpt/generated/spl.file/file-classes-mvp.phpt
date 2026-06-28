--TEST--
SPL generated file class MVP covers file metadata and line iteration
--FILE--
<?php
$info = new SplFileInfo(__FILE__);
echo $info->isFile() ? "file\n" : "not-file\n";
echo (new SplFileInfo(__DIR__))->isDir() ? "dir\n" : "not-dir\n";
if ($info->getFilename() !== '') {
    echo "filename\n";
} else {
    echo "no-filename\n";
}

$file = new SplFileObject(__FILE__);
echo ($file instanceof RecursiveIterator) ? "recursive\n" : "not-recursive\n";
echo ($file instanceof SeekableIterator) ? "seekable\n" : "not-seekable\n";
echo trim($file->fgets()), "\n";
$file->rewind();
foreach ($file as $key => $line) {
    echo "$key:", trim($line), "\n";
    break;
}

$temp = new SplTempFileObject();
echo $temp->getPathname(), '|', $temp->eof() ? 'eof' : 'not-eof', "\n";
?>
--EXPECT--
file
dir
filename
recursive
seekable
<?php
0:<?php
php://temp|not-eof

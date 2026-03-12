var path = "/tmp/lnsy_test_read.txt";
fs.writeFile(path, "read me back");
console.log(fs.readFile(path));
fs.deleteFile(path);

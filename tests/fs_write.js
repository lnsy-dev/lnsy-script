var path = "/tmp/lnsy_test_write.txt";
fs.writeFile(path, "hello file");
console.log(fs.readFile(path));
fs.deleteFile(path);
console.log(fs.exists(path));

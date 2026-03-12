var path = "/tmp/lnsy_test_delete.txt";
fs.writeFile(path, "temporary");
console.log(fs.exists(path));
fs.deleteFile(path);
console.log(fs.exists(path));

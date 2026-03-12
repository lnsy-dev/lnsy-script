var path = "/tmp/lnsy_test_append.txt";
fs.writeFile(path, "line1\n");
fs.appendFile(path, "line2\n");
fs.appendFile(path, "line3");
console.log(fs.readFile(path));
fs.deleteFile(path);

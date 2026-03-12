var path = "/tmp/lnsy_test.env";
fs.writeFile(path, [
    "# a comment",
    "APP_NAME=lnsy-script",
    'DB_URL="postgres://localhost/mydb"',
    "PORT=3000",
    "  SPACED  =  value with spaces  ",
    "",
    "QUOTED='hello world'",
].join('\n'));

var env = fs.readDotEnv(path);
console.log(JSON.stringify(env));
// Expected: APP_NAME, DB_URL, PORT, SPACED, QUOTED — no comment key

fs.deleteFile(path);

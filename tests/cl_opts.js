// cwd option
cl("pwd", { cwd: "/tmp" }).then(function(r) {
  var dir = r.stdout.trim();
  // /tmp may be a symlink on macOS (resolves to /private/tmp)
  console.log(dir === '/tmp' || dir === '/private/tmp');
  // env option
  return cl("echo $MY_CL_VAR", { env: { MY_CL_VAR: "hello_env" } });
}).then(function(r) {
  console.log(r.stdout.trim() === 'hello_env');
  // stdin option
  return cl("cat", { stdin: "piped input\n" });
}).then(function(r) {
  console.log(r.stdout.trim() === 'piped input');
}).catch(function(e) { console.log('error: ' + e.message); });

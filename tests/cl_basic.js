cl("echo hello").then(function(r) {
  console.log(r.stdout.trim() === 'hello');
  console.log(r.stderr === '');
  console.log(r.code === 0);
  console.log(r.success === true);
  console.log(typeof r.duration === 'number');
}).catch(function(e) { console.log('error: ' + e.message); });

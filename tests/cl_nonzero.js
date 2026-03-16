cl("exit 42").then(function(r) {
  console.log(r.code === 42);
  console.log(r.success === false);
}).catch(function(e) { console.log('error: ' + e.message); });

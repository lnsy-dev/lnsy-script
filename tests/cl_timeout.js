cl("sleep 30", { timeout: 400 }).then(function() {
  console.log('should not resolve');
}).catch(function(e) {
  console.log(e instanceof Error);
  console.log(e.message.indexOf('timed out') !== -1);
  console.log(e.message.indexOf('400ms') !== -1);
  console.log(e.result !== null && typeof e.result === 'object');
  console.log(typeof e.result.stdout === 'string');
  console.log(typeof e.result.stderr === 'string');
});

cl("__definitely_not_a_real_command_xyz__").then(function() {
  console.log('should not resolve');
}).catch(function(e) {
  console.log(e instanceof Error);
  console.log(e.message.length > 0);
  console.log(e.message.indexOf('not found') !== -1 || e.message.indexOf('__definitely') !== -1);
});

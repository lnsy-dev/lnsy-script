var received = false;
var a = new Agent('tests/fixtures/echo_worker.js');
a.onmessage = function(e) { received = true; console.log(e.data); };
a.postMessage('hello');
(function() { var attempts = 0; while (!received && attempts < 200) { __agentPoll(); attempts++; } })()

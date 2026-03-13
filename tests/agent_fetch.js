var received = false;
var a = new Agent('tests/fixtures/fetch_worker.js');
a.onmessage = function(e) { received = true; console.log(e.data); };
a.postMessage('go');
(function() { var attempts = 0; while (!received && attempts < 5000) { __agentPoll(); attempts++; } })()

// Use a command that emits one line every 100ms so the pipe buffer stays small.
let callCount = 0;
const r = await cl("while true; do echo running; sleep 0.1; done", {
  timeout: 5000,
  onStatus(s) {
    callCount++;
    if (callCount === 1) { s.kill(); }
  }
});
console.log(callCount >= 1);
console.log(r.stdout.trim().length > 0);
console.log(r.success === false);

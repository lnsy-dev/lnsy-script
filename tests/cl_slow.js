// Slow command: emits output in three bursts over ~1.5 seconds.
// Tests that onStatus fires incrementally and the final result is complete.
const statusTimes = [];
const start = Date.now();

const r = await cl("for i in 1 2 3; do sleep 0.5; echo step$i; done", {
  timeout: 10000,
  onStatus(s) { statusTimes.push(Date.now() - start); }
});

console.log(r.success === true);
console.log(r.stdout.indexOf('step1') !== -1);
console.log(r.stdout.indexOf('step2') !== -1);
console.log(r.stdout.indexOf('step3') !== -1);
// onStatus should have fired at least 3 times (once per echo line)
console.log(statusTimes.length >= 3);
// total duration should be at least 1000ms (three 0.5s sleeps)
console.log(r.duration >= 1000);

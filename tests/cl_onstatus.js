const chunks = [];
const r = await cl("printf 'line1\\nline2\\nline3\\n'", {
  onStatus(s) { chunks.push(s); }
});
console.log(chunks.length > 0);
console.log(chunks[0].stream === 'stdout');
console.log(typeof chunks[0].chunk === 'string');
console.log(typeof chunks[0].elapsed === 'number');
console.log(typeof chunks[0].kill === 'function');
console.log(r.stdout.indexOf('line1') !== -1);
console.log(r.stdout.indexOf('line3') !== -1);

self.onmessage = function(event) {
  fetch('https://httpbin.org/status/200').then(function(r) {
    self.postMessage(String(r.status));
  });
};

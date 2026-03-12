fetch("https://httpbin.org/status/200").then(function(r) {
    console.log(r.status)
    console.log(r.ok)
}).catch(function(e) { console.log("error: " + e) })

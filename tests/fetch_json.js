fetch("https://httpbin.org/json").then(function(r) {
    return r.json()
}).then(function(data) {
    console.log(typeof data)
    console.log(typeof data.slideshow)
}).catch(function(e) { console.log("error: " + e) })

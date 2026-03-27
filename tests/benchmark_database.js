var db = new Database();
var INSERT_COUNT = 500;
var QUERY_COUNT = 100;

// Insert phase
var insertStart = Date.now();
var chain = Promise.resolve();
for (var i = 0; i < INSERT_COUNT; i++) {
    (function(idx) {
        chain = chain.then(function() {
            return db.addItem({ name: "user" + idx, city: "city" + (idx % 10), score: idx });
        });
    })(i);
}

chain.then(function() {
    var insertMs = Date.now() - insertStart;
    console.log("addItem: " + (insertMs / INSERT_COUNT).toFixed(3) + "ms/op (" + INSERT_COUNT + " ops, " + insertMs + "ms total)");

    // find phase
    var findStart = Date.now();
    var findChain = Promise.resolve();
    for (var j = 0; j < QUERY_COUNT; j++) {
        findChain = findChain.then(function() {
            return db.find("name", "user42");
        });
    }
    return findChain.then(function() {
        var findMs = Date.now() - findStart;
        console.log("find:    " + (findMs / QUERY_COUNT).toFixed(3) + "ms/op (" + QUERY_COUNT + " ops, " + findMs + "ms total)");
    });
}).then(function() {
    // search phase
    var searchStart = Date.now();
    var searchChain = Promise.resolve();
    for (var k = 0; k < QUERY_COUNT; k++) {
        searchChain = searchChain.then(function() {
            return db.search("user");
        });
    }
    return searchChain.then(function() {
        var searchMs = Date.now() - searchStart;
        console.log("search:  " + (searchMs / QUERY_COUNT).toFixed(3) + "ms/op (" + QUERY_COUNT + " ops, " + searchMs + "ms total)");
    });
}).catch(function(err) {
    console.log("benchmark error: " + err);
});

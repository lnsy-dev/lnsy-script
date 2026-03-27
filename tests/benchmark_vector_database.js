var db = new VectorDatabase();
var INSERT_COUNT = 1000;
var QUERY_COUNT = 100;
var DIM = 384;

// Build a base embedding and a query embedding
function makeEmbedding(seed) {
    var arr = new Array(DIM);
    for (var i = 0; i < DIM; i++) {
        arr[i] = Math.sin(seed * 0.1 + i * 0.01);
    }
    return arr;
}

var queryEmb = makeEmbedding(42);

// Insert phase
var insertStart = Date.now();
var chain = Promise.resolve();
for (var i = 0; i < INSERT_COUNT; i++) {
    (function(idx) {
        chain = chain.then(function() {
            return db.addItem(makeEmbedding(idx), { label: "item" + idx });
        });
    })(i);
}

chain.then(function() {
    var insertMs = Date.now() - insertStart;
    console.log("addItem: " + (insertMs / INSERT_COUNT).toFixed(3) + "ms/op (" + INSERT_COUNT + " ops, " + insertMs + "ms total)");

    // query phase (cosine)
    var queryStart = Date.now();
    var queryChain = Promise.resolve();
    for (var j = 0; j < QUERY_COUNT; j++) {
        queryChain = queryChain.then(function() {
            return db.query(queryEmb, 10, "cosine");
        });
    }
    return queryChain.then(function() {
        var queryMs = Date.now() - queryStart;
        console.log("query (cosine):     " + (queryMs / QUERY_COUNT).toFixed(3) + "ms/op (" + QUERY_COUNT + " ops, " + queryMs + "ms total)");
    });
}).then(function() {
    // query phase (euclidean)
    var queryStart = Date.now();
    var queryChain = Promise.resolve();
    for (var j = 0; j < QUERY_COUNT; j++) {
        queryChain = queryChain.then(function() {
            return db.query(queryEmb, 10, "euclidean");
        });
    }
    return queryChain.then(function() {
        var queryMs = Date.now() - queryStart;
        console.log("query (euclidean):  " + (queryMs / QUERY_COUNT).toFixed(3) + "ms/op (" + QUERY_COUNT + " ops, " + queryMs + "ms total)");
    });
}).catch(function(err) {
    console.log("benchmark error: " + err);
});

var db = new GraphDatabase();
var NODE_COUNT = 500;
var EDGE_COUNT = 500;
var QUERY_COUNT = 100;

// Insert nodes
var nodeIds = [];
var nodeStart = Date.now();
var chain = Promise.resolve();
for (var i = 0; i < NODE_COUNT; i++) {
    (function(idx) {
        chain = chain.then(function() {
            return db.addNode({ name: "node" + idx, type: "person", score: idx });
        }).then(function(id) {
            nodeIds.push(id);
        });
    })(i);
}

chain.then(function() {
    var nodeMs = Date.now() - nodeStart;
    console.log("addNode: " + (nodeMs / NODE_COUNT).toFixed(3) + "ms/op (" + NODE_COUNT + " ops, " + nodeMs + "ms total)");

    // Insert edges
    var edgeStart = Date.now();
    var edgeChain = Promise.resolve();
    for (var j = 0; j < EDGE_COUNT; j++) {
        (function(idx) {
            edgeChain = edgeChain.then(function() {
                var src = nodeIds[idx % nodeIds.length];
                var tgt = nodeIds[(idx + 1) % nodeIds.length];
                return db.addEdge({ source: src, target: tgt, name: "knows" });
            });
        })(j);
    }
    return edgeChain.then(function() {
        var edgeMs = Date.now() - edgeStart;
        console.log("addEdge: " + (edgeMs / EDGE_COUNT).toFixed(3) + "ms/op (" + EDGE_COUNT + " ops, " + edgeMs + "ms total)");
    });
}).then(function() {
    // findNode phase
    var findStart = Date.now();
    var findChain = Promise.resolve();
    for (var k = 0; k < QUERY_COUNT; k++) {
        findChain = findChain.then(function() {
            return db.findNode({ name: "node42" });
        });
    }
    return findChain.then(function() {
        var findMs = Date.now() - findStart;
        console.log("findNode:          " + (findMs / QUERY_COUNT).toFixed(3) + "ms/op (" + QUERY_COUNT + " ops, " + findMs + "ms total)");
    });
}).then(function() {
    // getConnectedNodes phase
    var connStart = Date.now();
    var connChain = Promise.resolve();
    var targetId = nodeIds[0];
    for (var m = 0; m < QUERY_COUNT; m++) {
        connChain = connChain.then(function() {
            return db.getConnectedNodes(targetId);
        });
    }
    return connChain.then(function() {
        var connMs = Date.now() - connStart;
        console.log("getConnectedNodes: " + (connMs / QUERY_COUNT).toFixed(3) + "ms/op (" + QUERY_COUNT + " ops, " + connMs + "ms total)");
    });
}).catch(function(err) {
    console.log("benchmark error: " + err);
});

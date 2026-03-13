var db = new GraphDatabase();
db.addNode({name: "alice"}).then(function(id1) {
    console.log(typeof id1 === 'number');
    console.log(id1 >= 1);
    return db.addNode({name: "bob"});
}).then(function(id2) {
    return db.addEdge({source: 1, target: id2, name: "knows", meta: "friends"});
}).then(function(eid) {
    console.log(typeof eid === 'number');
    return db.getNode(1);
}).then(function(node) {
    console.log(node !== null);
    console.log(node.id === 1);
    return db.findNode({name: "alice"});
}).then(function(found) {
    console.log(found !== null);
    console.log(found.name === 'alice');
    console.log(typeof found.getConnectedNodes === 'function');
    return found.getConnectedNodes(found.id);
}).then(function(connected) {
    console.log(Array.isArray(connected));
    console.log(connected.length >= 1);
}).catch(function(err) {
    console.log("error: " + err);
});

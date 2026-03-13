use rquickjs::{class::Trace, Ctx, JsLifetime};
use rquickjs::prelude::Opt;
use sqlitegraph::{
    backend::{BackendDirection, GraphBackend, NeighborQuery, NodeSpec, EdgeSpec, SqliteGraphBackend},
    index::{add_property, get_entities_by_property},
    SqliteGraph, SnapshotId,
};
use std::cell::RefCell;

#[derive(Trace, JsLifetime)]
#[rquickjs::class]
pub struct GraphDatabase {
    #[qjs(skip_trace)]
    backend: RefCell<SqliteGraphBackend>,
}

#[rquickjs::methods]
impl GraphDatabase {
    #[qjs(constructor)]
    pub fn new(path: Opt<String>) -> rquickjs::Result<Self> {
        let backend = match path.0 {
            None => SqliteGraphBackend::in_memory()
                .map_err(|e| rquickjs::Error::new_from_js_message(
                    "error", "GraphDatabase", e.to_string()
                ))?,
            Some(p) => {
                let graph = SqliteGraph::open(&p)
                    .map_err(|e| rquickjs::Error::new_from_js_message(
                        "error", "GraphDatabase", e.to_string()
                    ))?;
                SqliteGraphBackend::from_graph(graph)
            }
        };
        Ok(GraphDatabase { backend: RefCell::new(backend) })
    }

    #[qjs(rename = "__addNodeSync")]
    pub fn add_node_sync(&self, json_str: String) -> rquickjs::Result<i64> {
        let data: serde_json::Value = serde_json::from_str(&json_str)
            .map_err(|e| rquickjs::Error::new_from_js_message("error", "addNode", e.to_string()))?;
        let name = data.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let spec = NodeSpec { kind: "node".to_string(), name, file_path: None, data: data.clone() };
        let id = self.backend.borrow().insert_node(spec)
            .map_err(|e| rquickjs::Error::new_from_js_message("error", "addNode", e.to_string()))?;
        // Register each JSON key-value as a searchable property for findNode
        if let Some(obj) = data.as_object() {
            let graph = self.backend.borrow();
            for (key, val) in obj {
                let val_str = match val {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                let _ = add_property(graph.graph(), id, key, &val_str);
            }
        }
        Ok(id)
    }

    #[qjs(rename = "__addEdgeSync")]
    pub fn add_edge_sync(&self, json_str: String) -> rquickjs::Result<i64> {
        let data: serde_json::Value = serde_json::from_str(&json_str)
            .map_err(|e| rquickjs::Error::new_from_js_message("error", "addEdge", e.to_string()))?;
        let source = data.get("source").and_then(|v| v.as_i64())
            .ok_or_else(|| rquickjs::Error::new_from_js_message(
                "error", "addEdge", "missing 'source' field".to_string()
            ))?;
        let target = data.get("target").and_then(|v| v.as_i64())
            .ok_or_else(|| rquickjs::Error::new_from_js_message(
                "error", "addEdge", "missing 'target' field".to_string()
            ))?;
        let edge_type = data.get("name").and_then(|v| v.as_str()).unwrap_or("edge").to_string();
        let spec = EdgeSpec { from: source, to: target, edge_type, data };
        self.backend.borrow().insert_edge(spec)
            .map_err(|e| rquickjs::Error::new_from_js_message("error", "addEdge", e.to_string()))
    }

    #[qjs(rename = "__getNodeSync")]
    pub fn get_node_sync(&self, id: i64) -> rquickjs::Result<String> {
        let snapshot = SnapshotId::current();
        let entity = self.backend.borrow().get_node(snapshot, id)
            .map_err(|e| rquickjs::Error::new_from_js_message("error", "getNode", e.to_string()))?;
        serde_json::to_string(&serde_json::json!({
            "id": entity.id,
            "kind": entity.kind,
            "name": entity.name,
            "data": entity.data,
        })).map_err(|e| rquickjs::Error::new_from_js_message("error", "getNode", e.to_string()))
    }

    #[qjs(rename = "__findNodeSync")]
    pub fn find_node_sync(&self, key: String, value: String) -> rquickjs::Result<String> {
        let entities = get_entities_by_property(self.backend.borrow().graph(), &key, &value)
            .map_err(|e| rquickjs::Error::new_from_js_message("error", "findNode", e.to_string()))?;
        match entities.into_iter().next() {
            Some(entity) => serde_json::to_string(&serde_json::json!({
                "id": entity.id,
                "kind": entity.kind,
                "name": entity.name,
                "data": entity.data,
            })).map_err(|e| rquickjs::Error::new_from_js_message("error", "findNode", e.to_string())),
            None => Ok("null".to_string()),
        }
    }

    #[qjs(rename = "__getConnectedNodesSync")]
    pub fn get_connected_nodes_sync(&self, node_id: i64) -> rquickjs::Result<String> {
        let snapshot = SnapshotId::current();
        let query = NeighborQuery { direction: BackendDirection::Outgoing, edge_type: None };
        let neighbor_ids = self.backend.borrow().neighbors(snapshot, node_id, query)
            .map_err(|e| rquickjs::Error::new_from_js_message(
                "error", "getConnectedNodes", e.to_string()
            ))?;
        let mut nodes = Vec::new();
        for nid in neighbor_ids {
            let snap = SnapshotId::current();
            if let Ok(entity) = self.backend.borrow().get_node(snap, nid) {
                nodes.push(serde_json::json!({
                    "id": entity.id,
                    "kind": entity.kind,
                    "name": entity.name,
                    "data": entity.data,
                }));
            }
        }
        serde_json::to_string(&nodes)
            .map_err(|e| rquickjs::Error::new_from_js_message(
                "error", "getConnectedNodes", e.to_string()
            ))
    }
}

pub fn setup_graph_database(ctx: Ctx<'_>) -> rquickjs::Result<()> {
    rquickjs::Class::<GraphDatabase>::define(&ctx.globals())?;
    ctx.eval::<(), _>(r#"
GraphDatabase.prototype.addNode = function(obj) {
    var self = this;
    return new Promise(function(resolve, reject) {
        try { resolve(self.__addNodeSync(JSON.stringify(obj))); }
        catch(e) { reject(e); }
    });
};
GraphDatabase.prototype.addEdge = function(obj) {
    var self = this;
    return new Promise(function(resolve, reject) {
        try { resolve(self.__addEdgeSync(JSON.stringify(obj))); }
        catch(e) { reject(e); }
    });
};
GraphDatabase.prototype.getNode = function(id) {
    var self = this;
    return new Promise(function(resolve, reject) {
        try {
            var raw = self.__getNodeSync(id);
            resolve(raw === "null" ? null : JSON.parse(raw));
        }
        catch(e) { reject(e); }
    });
};
GraphDatabase.prototype.findNode = function(obj) {
    var self = this;
    return new Promise(function(resolve, reject) {
        try {
            var keys = Object.keys(obj);
            if (keys.length === 0) { reject(new Error("findNode: empty query")); return; }
            var key = keys[0];
            var value = String(obj[key]);
            var raw = self.__findNodeSync(key, value);
            if (raw === "null") { resolve(null); return; }
            var node = JSON.parse(raw);
            node.getConnectedNodes = function(id) {
                return new Promise(function(resolve, reject) {
                    try { resolve(JSON.parse(self.__getConnectedNodesSync(id))); }
                    catch(e) { reject(e); }
                });
            };
            resolve(node);
        }
        catch(e) { reject(e); }
    });
};
GraphDatabase.prototype.getConnectedNodes = function(id) {
    var self = this;
    return new Promise(function(resolve, reject) {
        try { resolve(JSON.parse(self.__getConnectedNodesSync(id))); }
        catch(e) { reject(e); }
    });
};
    "#)?;
    Ok(())
}

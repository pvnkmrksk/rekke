//! The MCP server, driven the way an agent's client drives it: the real
//! binary over stdio, one JSON-RPC message per line.
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};

struct Client {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next: u64,
}
impl Client {
    fn start(directory: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_rekkefly"))
            .args(["mcp", "--config-dir"])
            .arg(directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            stdin,
            stdout,
            next: 1,
        }
    }
    fn send(&mut self, message: &Value) {
        writeln!(self.stdin, "{message}").unwrap();
        self.stdin.flush().unwrap();
    }
    fn receive(&mut self) -> Value {
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("{e}: {line:?}"))
    }
    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        let reply = self.receive();
        assert_eq!(reply["jsonrpc"], "2.0");
        assert_eq!(reply["id"], id);
        reply
    }
    /// A tool's structured result; panics if the tool reports an error.
    fn call(&mut self, name: &str, arguments: Value) -> Value {
        let reply = self.request("tools/call", json!({"name": name, "arguments": arguments}));
        let result = &reply["result"];
        assert_eq!(result["isError"], false, "{name}: {reply}");
        result["structuredContent"].clone()
    }
    fn fails(&mut self, name: &str, arguments: Value) -> String {
        let reply = self.request("tools/call", json!({"name": name, "arguments": arguments}));
        assert_eq!(
            reply["result"]["isError"], true,
            "{name} should fail: {reply}"
        );
        reply["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_owned()
    }
}
fn moth() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("samples/bogong-moth.avi")
        .to_string_lossy()
        .into_owned()
}

#[test]
fn an_agent_can_configure_run_watch_and_stop_a_rig() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = Client::start(dir.path());
    // Handshake: the client's protocol version is accepted.
    let init = client.request(
        "initialize",
        json!({"protocolVersion": "2025-06-18", "capabilities": {},
               "clientInfo": {"name": "test", "version": "1"}}),
    );
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(init["result"]["serverInfo"]["name"], "rekkefly");
    assert!(init["result"]["capabilities"]["tools"].is_object());
    assert!(
        init["result"]["instructions"]
            .as_str()
            .unwrap()
            .contains("L−R")
    );
    client.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    assert_eq!(client.request("ping", json!({}))["result"], json!({}));
    let tools = client.request("tools/list", json!({}));
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    for expected in [
        "start_rig",
        "rig_status",
        "snapshot",
        "update_config",
        "track_video",
    ] {
        assert!(names.contains(&expected), "{names:?}");
    }

    // Make a quiet rig config, then move its head marker.
    let made = client.call(
        "update_config",
        json!({"config": "arena-1", "create": true,
               "changes": {"output": {"enabled": false, "zmq_enabled": false}}}),
    );
    assert_eq!(made["config"]["instance"], "arena-1");
    assert!(dir.path().join("arena-1.toml").is_file());
    let moved = client.call(
        "update_config",
        json!({"config": "arena-1", "changes": {"head": {"x": 318.0}}}),
    );
    assert_eq!(moved["config"]["head"]["x"], 318.0);
    // Invalid values are rejected and nothing is written.
    let error = client.fails(
        "update_config",
        json!({"config": "arena-1", "changes": {"tracking": {"left": {"span_deg": 999}}}}),
    );
    assert!(error.contains("span"), "{error}");
    let error = client.fails(
        "update_config",
        json!({"config": "arena-1", "changes": {"tracking": {"left": {"spike_width": 5}}}}),
    );
    assert!(error.contains("spike_width"), "typos are named: {error}");
    let listed = client.call("list_configs", json!({}));
    assert_eq!(listed["configs"][0]["status"], "available");

    // Run it on the simulated fly, watch it, look at it, stop it.
    let started = client.call(
        "start_rig",
        json!({"config": "arena-1", "demo": true, "label": "agent-run"}),
    );
    assert_eq!(started["rig"], "agent-run");
    assert!(started["error"].is_null(), "{started}");
    let status = client.call("rig_status", json!({"rig": "agent-run"}));
    assert!(status["angles_deg"]["left"].is_number(), "{status}");
    assert!(status["frames"].as_u64().unwrap() > 0);
    let listed = client.call("list_configs", json!({}));
    assert_eq!(listed["configs"][0]["status"], "running here as agent-run");
    let reply = client.request(
        "tools/call",
        json!({"name": "snapshot", "arguments": {"rig": "agent-run", "width": 320}}),
    );
    let image = &reply["result"]["content"][0];
    assert_eq!(image["type"], "image");
    assert_eq!(image["mimeType"], "image/jpeg");
    assert!(
        image["data"].as_str().unwrap().starts_with("/9j/"),
        "a JPEG"
    );
    // Every frame of a one-second trial, summarised and saved.
    let csv = dir.path().join("trial.csv");
    let trial = client.call(
        "measure",
        json!({"rig": "agent-run", "seconds": 1.0, "csv": csv}),
    );
    let frames = trial["frames"].as_u64().unwrap();
    assert!(frames >= 20, "{trial}");
    assert_eq!(trial["skipped_frames"], 0);
    assert_eq!(trial["angles_deg"]["left_minus_right"]["valid"], frames);
    let rows = std::fs::read_to_string(&csv).unwrap().lines().count() as u64;
    assert_eq!(rows, frames + 1, "a header and one row per frame");
    // A live change reaches the running rig at once.
    let applied = client.call(
        "update_config",
        json!({"config": "arena-1", "changes": {"tracking": {"left": {"spike_width_deg": 6.0}}}}),
    );
    assert!(
        applied["note"].as_str().unwrap().contains("applied"),
        "{applied}"
    );
    let stopped = client.call("stop_rig", json!({"rig": "agent-run"}));
    assert_eq!(stopped["stopped"][0]["running"], false);
    let error = client.fails("rig_status", json!({"rig": "agent-run"}));
    assert!(error.contains("No rig"), "{error}");

    // Offline: track, look at and cut the moth video.
    let summary = client.call("track_video", json!({"video": moth(), "frames": 60}));
    assert_eq!(summary["frames"], 60);
    assert_eq!(summary["video_markers"], true);
    assert!(summary["left_minus_right"]["mean"].is_number());
    let shot = client.call("snapshot", json!({"video": moth(), "frame": 12}));
    assert_eq!(shot["own_markers"], true);
    assert!(shot["angles_deg"]["right"].is_number());
    let out = dir.path().join("cut.avi");
    let cut = client.call(
        "clip_video",
        json!({"input": moth(), "output": out, "duration": 1.0, "quality": 60}),
    );
    assert_eq!(cut["frames"], 30);
    let error = client.fails("clip_video", json!({"input": moth(), "output": out}));
    assert!(error.contains("already exists"), "{error}");

    // Protocol errors: unknown tools, unknown arguments, unknown methods.
    let reply = client.request("tools/call", json!({"name": "fly_away", "arguments": {}}));
    assert_eq!(reply["error"]["code"], -32602);
    let reply = client.request(
        "tools/call",
        json!({"name": "rig_status", "arguments": {"rigg": "x"}}),
    );
    assert!(reply["error"]["message"].as_str().unwrap().contains("rigg"));
    assert_eq!(
        client.request("resources/read", json!({}))["error"]["code"],
        -32601
    );
    client.send(&json!("not a request"));
    assert_eq!(client.receive()["error"]["code"], -32600);
    writeln!(client.stdin, "{{not json").unwrap();
    client.stdin.flush().unwrap();
    assert_eq!(client.receive()["error"]["code"], -32700);

    // Closing stdin ends the server cleanly.
    drop(client.stdin);
    assert!(client.child.wait().unwrap().success());
}
#[test]
fn unknown_protocol_versions_get_the_newest() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = Client::start(dir.path());
    let init = client.request("initialize", json!({"protocolVersion": "1999-01-01"}));
    assert_eq!(
        init["result"]["protocolVersion"],
        rekkefly::mcp::PROTOCOL_VERSIONS[0]
    );
    // A batch gets one reply per request.
    client.send(&json!([
        {"jsonrpc": "2.0", "id": "a", "method": "ping"},
        {"jsonrpc": "2.0", "method": "notifications/initialized"},
        {"jsonrpc": "2.0", "id": "b", "method": "tools/list"}
    ]));
    let replies = client.receive();
    assert_eq!(replies.as_array().unwrap().len(), 2);
    drop(client.stdin);
    assert!(client.child.wait().unwrap().success());
}

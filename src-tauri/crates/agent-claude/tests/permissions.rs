//! The permission round trip against a stand-in `claude`: it asks to run a
//! command over stdout, and records what deck answers on stdin.

use agent_claude::ClaudeAdapter;
use agent_core::{AgentAdapter, AgentEvent, Decision, PermissionMode, SessionCtx, UserTurn};
use futures::StreamExt;
use std::time::Duration;

#[tokio::test]
async fn an_approval_goes_back_as_a_control_response() {
    let dir = std::env::temp_dir().join(format!("deck-perm-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let log = dir.join("stdin.log");
    let args = dir.join("args.log");
    let fake = dir.join("claude");
    std::fs::write(
        &fake,
        format!(
            r#"#!/bin/sh
echo "$@" > '{args}'
read first
echo "$first" >> '{log}'
echo '{{"type":"system","subtype":"init","session_id":"s-1"}}'
echo '{{"type":"control_request","request_id":"req-7","request":{{"subtype":"can_use_tool","tool_name":"Bash","input":{{"command":"touch a"}},"permission_suggestions":[{{"type":"addRules","rules":[{{"toolName":"Bash","ruleContent":"touch a"}}],"behavior":"allow","destination":"localSettings"}}],"tool_use_id":"tu_1"}}}}'
read answer
echo "$answer" >> '{log}'
echo '{{"type":"result","subtype":"success","result":"ok"}}'
sleep 1
"#,
            args = args.display(),
            log = log.display()
        ),
    )
    .unwrap();
    std::process::Command::new("chmod")
        .arg("+x")
        .arg(&fake)
        .status()
        .unwrap();

    let adapter = ClaudeAdapter::with_bin(&fake);
    let ctx = SessionCtx {
        session_id: agent_core::SessionId::new(),
        cwd: dir.clone(),
        system_prompt: "".into(),
        model: None,
        mcp: Default::default(),
        skills_dir: None,
        runtime_dir: dir.join("rt"),
        resume: None,
        permission_mode: PermissionMode::Ask,
        env: Vec::new(),
        allowed_tools: Vec::new(),
    };
    let mut session = adapter.spawn(ctx).await.unwrap();
    session.send(UserTurn::text("go")).await.unwrap();

    let id = tokio::time::timeout(Duration::from_secs(5), async {
        let mut events = session.events();
        while let Some(ev) = events.next().await {
            if let AgentEvent::Permission { id, .. } = ev {
                return id;
            }
        }
        panic!("no permission request");
    })
    .await
    .unwrap();
    session.decide(id, Decision::AllowAlways).await.unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;

    let args = std::fs::read_to_string(&args).unwrap();
    assert!(args.contains("--permission-prompt-tool stdio"), "{args}");
    assert!(args.contains("--permission-mode default"), "{args}");
    let lines: Vec<serde_json::Value> = std::fs::read_to_string(&log)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let answer = &lines[1];
    assert_eq!(answer["type"], "control_response");
    assert_eq!(answer["response"]["request_id"], "req-7");
    let r = &answer["response"]["response"];
    assert_eq!(r["behavior"], "allow");
    assert_eq!(r["updatedInput"]["command"], "touch a");
    assert_eq!(r["updatedPermissions"][0]["destination"], "session");
    let _ = std::fs::remove_dir_all(&dir);
}

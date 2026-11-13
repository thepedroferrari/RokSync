use crate::api::lan_client;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub struct BridgeInfo {
    pub name: String,
    pub bridge_id: String,
    pub model_id: String,
    pub sw_version: String,
    pub api_version: String,
}

#[derive(Debug, Clone)]
pub struct PairResult {
    /// a.k.a. "username"; sent as `hue-application-key` and used as the DTLS PSK identity.
    pub app_key: String,
    /// hex PSK for Entertainment streaming.
    pub client_key: String,
}

/// Unauthenticated bridge info.
pub fn bridge_info(ip: &str) -> Result<BridgeInfo> {
    let v: Value = lan_client()?
        .get(format!("https://{ip}/api/0/config"))
        .send()
        .with_context(|| format!("no Hue bridge answering at {ip}"))?
        .json()?;
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or_default().to_string();
    Ok(BridgeInfo {
        name: s("name"),
        bridge_id: s("bridgeid"),
        model_id: s("modelid"),
        sw_version: s("swversion"),
        api_version: s("apiversion"),
    })
}

/// One pairing attempt. Returns Ok(None) while the link button hasn't been pressed.
pub fn try_pair(ip: &str, device_type: &str) -> Result<Option<PairResult>> {
    let v: Value = lan_client()?
        .post(format!("https://{ip}/api"))
        .json(&json!({ "devicetype": device_type, "generateclientkey": true }))
        .send()?
        .json()?;
    let first = v.get(0).cloned().unwrap_or(Value::Null);
    if let Some(ok) = first.get("success") {
        let app_key = ok.get("username").and_then(|x| x.as_str()).unwrap_or_default().to_string();
        let client_key = ok.get("clientkey").and_then(|x| x.as_str()).unwrap_or_default().to_string();
        if app_key.is_empty() || client_key.is_empty() {
            bail!("bridge paired but returned no key: {first}");
        }
        return Ok(Some(PairResult { app_key, client_key }));
    }
    if let Some(err) = first.get("error") {
        if err.get("type").and_then(|t| t.as_i64()) == Some(101) {
            return Ok(None);
        }
        bail!("pairing refused: {}", err.get("description").and_then(|d| d.as_str()).unwrap_or("unknown"));
    }
    bail!("unexpected pairing response: {v}")
}

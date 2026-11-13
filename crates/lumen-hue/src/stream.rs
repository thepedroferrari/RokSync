//! Hue Entertainment API v2 streaming: DTLS 1.2 PSK over UDP 2100, up to 20 channels per frame.

use crate::{api::Bridge, color::Rgb};
use anyhow::{anyhow, Context, Result};
use std::{
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::net::UdpSocket;
use webrtc_dtls::{cipher_suite::CipherSuiteId, config::Config, conn::DTLSConn};

pub struct Streamer {
    rt: tokio::runtime::Runtime,
    conn: Arc<DTLSConn>,
    bridge: Bridge,
    config_id: String,
    seq: u8,
    last: Vec<u8>,
    buf: Vec<u8>,
    repeats: u32,
    sent_at: Option<Instant>,
}

/// An unchanged frame still goes out this many times in a row, so one lost UDP packet never
/// leaves a light on an old colour.
const REPEATS: u32 = 3;
/// While nothing changes, one packet this often keeps the area alive (the bridge ends a stream
/// after ~10 s of silence) and re-sends the colours.
const KEEPALIVE: Duration = Duration::from_millis(200);

/// Changes always go out at once; only identical repeats are thinned.
fn due(changed: bool, repeats: u32, since_last: Option<Duration>) -> bool {
    changed || repeats < REPEATS || since_last.map_or(true, |d| d >= KEEPALIVE)
}

/// Channel id + 16-bit RGB per channel, exactly as on the wire (max 20 channels).
fn channel_bytes(channels: &[(u8, Rgb)], out: &mut Vec<u8>) {
    for (id, c) in channels.iter().take(20) {
        out.push(*id);
        for v in [c.r, c.g, c.b] {
            let v = (v.clamp(0.0, 1.0) * 65535.0).round() as u16;
            out.extend_from_slice(&v.to_be_bytes());
        }
    }
}

impl Streamer {
    /// Activates the entertainment area and opens the DTLS session.
    pub fn start(bridge: &Bridge, client_key_hex: &str, config_id: &str) -> Result<Self> {
        let psk = hex::decode(client_key_hex.trim()).context("client key is not hex")?;
        let identity = bridge.app_key.clone().into_bytes();
        let addr: SocketAddr = format!("{}:2100", bridge.ip).parse()?;

        bridge
            .entertainment_action(config_id, true)
            .context("bridge won't start the entertainment area. Close Hue Sync or any other app that's streaming")?;

        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
        let conn = rt.block_on(async move {
            let sock = UdpSocket::bind("0.0.0.0:0").await?;
            sock.connect(addr).await?;
            let psk_cb: Arc<dyn Fn(&[u8]) -> Result<Vec<u8>, webrtc_dtls::Error> + Send + Sync> =
                Arc::new(move |_hint: &[u8]| Ok(psk.clone()));
            let cfg = Config {
                psk: Some(psk_cb),
                psk_identity_hint: Some(identity),
                cipher_suites: vec![CipherSuiteId::Tls_Psk_With_Aes_128_Gcm_Sha256],
                ..Default::default()
            };
            let c = tokio::time::timeout(Duration::from_secs(6), DTLSConn::new(Arc::new(sock), cfg, true, None))
                .await
                .map_err(|_| anyhow!("secure link to the bridge timed out"))?
                .map_err(|e| anyhow!("secure link to the bridge failed: {e}"))?;
            anyhow::Ok(Arc::new(c))
        });
        let conn = match conn {
            Ok(c) => c,
            Err(e) => {
                let _ = bridge.entertainment_action(config_id, false);
                return Err(e);
            }
        };
        Ok(Self { rt, conn, bridge: bridge.clone(), config_id: config_id.to_string(), seq: 0, last: Vec::new(), buf: Vec::new(), repeats: 0, sent_at: None })
    }

    /// Send one frame. A frame identical on the wire to the last one is skipped once it has
    /// gone out `REPEATS` times, except for a `KEEPALIVE` packet; changes are never delayed.
    pub fn send(&mut self, channels: &[(u8, Rgb)]) -> Result<()> {
        let mut buf = std::mem::take(&mut self.buf);
        buf.clear();
        buf.extend_from_slice(b"HueStream");
        // version 2.0, sequence, reserved x2, colour space RGB, reserved
        buf.extend_from_slice(&[0x02, 0x00, self.seq, 0x00, 0x00, 0x00, 0x00]);
        buf.extend_from_slice(self.config_id.as_bytes());
        let head = buf.len();
        channel_bytes(channels, &mut buf);
        let changed = buf[head..] != self.last[..];
        if !due(changed, self.repeats, self.sent_at.map(|t| t.elapsed())) {
            self.buf = buf;
            return Ok(());
        }
        self.seq = self.seq.wrapping_add(1);
        let conn = self.conn.clone();
        let buf = self
            .rt
            .block_on(async move { conn.write(&buf, None).await.map(|_| buf) })
            .map_err(|e| anyhow!("couldn't send to the bridge: {e}"))?;
        self.repeats = if changed { 1 } else { self.repeats.saturating_add(1) };
        self.last.clear();
        self.last.extend_from_slice(&buf[head..]);
        self.buf = buf;
        self.sent_at = Some(Instant::now());
        Ok(())
    }
}

impl Drop for Streamer {
    fn drop(&mut self) {
        let conn = self.conn.clone();
        let _ = self.rt.block_on(async move { conn.close().await });
        let _ = self.bridge.entertainment_action(&self.config_id, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_go_out_at_once_repeats_are_thinned() {
        let ms = |n| Some(Duration::from_millis(n));
        assert!(due(true, 0, None));
        assert!(due(true, 99, ms(1)));
        assert!(due(false, 1, ms(20)) && due(false, 2, ms(20)));
        assert!(!due(false, REPEATS, ms(20)));
        assert!(due(false, REPEATS, ms(200)));
        assert!(due(false, 50, None));
    }

    #[test]
    fn channel_bytes_match_the_wire_format() {
        let mut out = Vec::new();
        channel_bytes(&[(3, Rgb::new(1.0, 0.0, 2.0))], &mut out);
        assert_eq!(out, [3, 0xff, 0xff, 0, 0, 0xff, 0xff]);
        // tiny float noise below one 16-bit step is the same packet
        let (mut a, mut b) = (Vec::new(), Vec::new());
        channel_bytes(&[(1, Rgb::new(0.5, 0.25, 0.125))], &mut a);
        channel_bytes(&[(1, Rgb::new(0.5 + 1e-7, 0.25, 0.125))], &mut b);
        assert_eq!(a, b);
    }
}

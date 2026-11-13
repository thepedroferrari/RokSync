use crate::catalog::{self, ModelSpec};
use crate::plan::{describe_scope, schema, scope_lights, MoodPlan, RawPlan};
use anyhow::{anyhow, bail, Context, Result};
use lumen_hue::Inventory;
use serde_json::{json, Value};
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const SYSTEM: &str = "You are a lighting designer. The user names anything: a place, country, film, game, brand, \
season, moment or feeling. You light their room so it instantly reads as that thing.\n\
- evokes: first, think. Name the 3-4 most iconic things the description brings to mind and the real colour of each. \
Pick what fits the subject: a country has a flag and landscapes, a brand has a logo, a film or game has its look, \
a feeling has a time of day and weather. Use what you know about the world.\n\
- palette: 3-6 plain colour words copied from evokes, the most recognisable first (like red, canary yellow, navy, \
hot pink, emerald, turquoise, warm white). Someone walking in should guess the description from the colours alone. \
Never repeat a colour. Leave out black, it can't be lit.\n\
- brightness: sleep or cinema 3-20, calm 15-40, cozy 30-60, focus 75-100, party or action 70-100.\n\
- focus: the light the user wants emphasised if they name or imply one, otherwise \"none\".\n\
- motion: still for most moods, breathe for calm or tense moods, drift for dreamy or flowing ones, flicker for fire, candles, storms and explosions. speed matches the energy.\n\
- title: a short, original, evocative name for this specific scene (2-4 words).";

// Few-shot examples, sent as prior user/assistant turns.
const EXAMPLES: &[(&str, &str)] = &[
    (
        "Jamaica",
        r#"{"evokes":"green, gold and black flag; red, gold and green reggae colours; turquoise Caribbean sea; golden sun","palette":["green","gold","turquoise","red"],"title":"Island Riddim","brightness":70,"focus":"none","motion":"drift","speed":"slow"}"#,
    ),
    (
        "Star Wars",
        r#"{"evokes":"red and blue lightsabers; orange desert suns of Tatooine; white stormtrooper armour; deep blue space","palette":["red","electric blue","orange","cool white","navy"],"title":"Saber Duel","brightness":55,"focus":"none","motion":"still","speed":"slow"}"#,
    ),
    (
        "a quiet rainy evening",
        r#"{"evokes":"grey-blue sky; wet streets under pale streetlights; a warm lamp indoors; dusk","palette":["storm blue","ice blue","candlelight","navy"],"title":"Window Weather","brightness":30,"focus":"none","motion":"breathe","speed":"slow"}"#,
    ),
];

/// Ties llama-server's lifetime to this process: when RokSync exits for any reason (quit, crash,
/// Task Manager), Windows closes this job handle and JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE kills the server.
#[cfg(windows)]
struct KillWithUs(isize);

#[cfg(windows)]
impl KillWithUs {
    fn attach(child: &Child) -> Option<Self> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };
        // SAFETY: a fresh job handle, a zeroed plain-data struct of the size we pass, and the
        // child's own process handle, which is valid while `child` lives.
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return None;
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let size = std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32;
            let ok = SetInformationJobObject(job, JobObjectExtendedLimitInformation, &info as *const _ as _, size) != 0
                && AssignProcessToJobObject(job, child.as_raw_handle() as _) != 0;
            if !ok {
                windows_sys::Win32::Foundation::CloseHandle(job);
                return None;
            }
            Some(Self(job as isize))
        }
    }
}

#[cfg(windows)]
impl Drop for KillWithUs {
    fn drop(&mut self) {
        // SAFETY: our own job handle, closed exactly once.
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.0 as _) };
    }
}

pub struct LlmServer {
    child: Child,
    #[cfg(windows)]
    _job: Option<KillWithUs>,
    port: u16,
    pub model: &'static ModelSpec,
    pub opts: RunOpts,
    http: reqwest::blocking::Client,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunOpts {
    pub gpu: bool,
    /// CPU threads; None lets llama.cpp decide.
    pub threads: Option<u32>,
}

impl Default for RunOpts {
    fn default() -> Self {
        Self { gpu: true, threads: None }
    }
}

fn free_port() -> Result<u16> {
    Ok(TcpListener::bind("127.0.0.1:0")?.local_addr()?.port())
}

impl LlmServer {
    /// Starts llama-server on 127.0.0.1 and waits until the model is loaded.
    pub fn start(model: &'static ModelSpec) -> Result<Self> {
        Self::start_with(model, RunOpts::default())
    }

    pub fn start_with(model: &'static ModelSpec, opts: RunOpts) -> Result<Self> {
        let exe = catalog::server_exe().ok_or_else(|| anyhow!("AI runtime not installed"))?;
        if !catalog::model_installed(model) {
            bail!("model {} not downloaded", model.name);
        }
        let port = free_port()?;
        let mut cmd = Command::new(&exe);
        cmd.arg("-m")
            .arg(catalog::model_path(model))
            .args(["--host", "127.0.0.1", "--port", &port.to_string()])
            .args(["-c", "4096", "--parallel", "1"])
            .args(["-ngl", if opts.gpu { "99" } else { "0" }]);
        if !opts.gpu {
            // don't even open the Vulkan device, so no VRAM is touched
            cmd.args(["--device", "none"]);
        }
        if let Some(t) = opts.threads {
            cmd.args(["-t", &t.to_string()]);
        }
        cmd.current_dir(exe.parent().unwrap())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let child = cmd.spawn().with_context(|| format!("starting {}", exe.display()))?;
        let http = reqwest::blocking::Client::builder().timeout(Duration::from_secs(120)).build()?;
        #[cfg(windows)]
        let _job = KillWithUs::attach(&child);
        let mut s = Self {
            child,
            #[cfg(windows)]
            _job,
            port,
            model,
            opts,
            http,
        };
        let deadline = Instant::now() + Duration::from_secs(90);
        loop {
            if let Ok(Some(status)) = s.child.try_wait() {
                bail!("AI runtime exited during start ({status})");
            }
            let ok = s
                .http
                .get(format!("http://127.0.0.1:{}/health", s.port))
                .timeout(Duration::from_secs(2))
                .send()
                .map(|r| r.status().is_success())
                .unwrap_or(false);
            if ok {
                return Ok(s);
            }
            if Instant::now() > deadline {
                bail!("AI runtime didn't become ready in 90 s");
            }
            std::thread::sleep(Duration::from_millis(300));
        }
    }

    /// `scope`: a room or zone name to light, or None for the whole home.
    pub fn ask(&self, prompt: &str, inv: &Inventory, scope: Option<&str>) -> Result<MoodPlan> {
        let (home, targets) = describe_scope(inv, scope);
        let raw = self.direct(prompt, &home, &targets)?;
        let plan = raw.into_plan(&scope_lights(inv, scope));
        if plan.assignments.is_empty() {
            bail!("no lights in that area");
        }
        Ok(plan)
    }

    /// The model's art direction for `prompt`, before it's placed on lights.
    pub fn direct(&self, prompt: &str, home: &str, targets: &[String]) -> Result<RawPlan> {
        self.direct_with(prompt, home, targets, true)
    }

    /// `notes_on: false` leaves out the fact sheet and mood hints.
    pub fn direct_with(&self, prompt: &str, home: &str, targets: &[String], notes_on: bool) -> Result<RawPlan> {
        // each user turn carries the known colours for subjects and mood words in its description
        let notes = |d: &str| {
            let mut s = String::new();
            if !notes_on {
                return s;
            }
            let facts = crate::facts::brief(d);
            if !facts.is_empty() {
                s.push_str(&format!("\nTrue colours (start the palette with these):\n{facts}"));
            }
            if let Some(p) = crate::keywords::hint(d) {
                let mut names: Vec<&str> = p.iter().map(|h| crate::colors::nearest_name(h)).collect();
                names.dedup();
                if facts.is_empty() {
                    s.push_str(&format!("\nColours that suit the mood: {}\n", names.join(", ")));
                } else {
                    s.push_str(&format!("Then add one or two accents for the setting, such as: {}\n", names.join(", ")));
                }
            }
            s
        };
        let user = |d: &str| format!("Description: {d}\n{}\nTheir lights:\n{home}", notes(d));
        let mut messages = vec![json!({ "role": "system", "content": SYSTEM })];
        for (d, a) in EXAMPLES {
            messages.push(json!({ "role": "user", "content": user(d) }));
            messages.push(json!({ "role": "assistant", "content": a }));
        }
        messages.push(json!({ "role": "user", "content": user(prompt) }));
        let body = json!({
            "messages": messages,
            "temperature": self.model.temperature,
            "max_tokens": 400,
            "seed": 7,
            "chat_template_kwargs": { "enable_thinking": false },
            "response_format": {
                "type": "json_schema",
                "json_schema": { "name": "lighting_plan", "strict": true, "schema": schema(targets) }
            }
        });
        let v: Value = self
            .http
            .post(format!("http://127.0.0.1:{}/v1/chat/completions", self.port))
            .json(&body)
            .send()?
            .error_for_status()?
            .json()?;
        let choice = &v["choices"][0];
        let text = choice["message"]["content"].as_str().unwrap_or_default();
        if text.trim().is_empty() {
            let why = choice["finish_reason"].as_str().unwrap_or("?");
            let thought: String = choice["message"]["reasoning_content"].as_str().unwrap_or_default().chars().take(200).collect();
            bail!("empty answer (finish: {why}) {thought}");
        }
        serde_json::from_str(text).with_context(|| format!("model returned invalid plan: {text}"))
    }
}

impl Drop for LlmServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

use crate::data_dir;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub file: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub bytes: u64,
    pub license: &'static str,
    pub blurb: &'static str,
    pub temperature: f32,
    /// Not offered for download, but still usable (and removable) if already installed.
    pub retired: bool,
}

pub const MODELS: &[ModelSpec] = &[
    ModelSpec {
        id: "qwen3.5-4b",
        name: "Qwen3.5 4B (recommended)",
        file: "Qwen3.5-4B-Q4_K_M.gguf",
        url: "https://huggingface.co/unsloth/Qwen3.5-4B-GGUF/resolve/main/Qwen3.5-4B-Q4_K_M.gguf",
        sha256: "00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4",
        bytes: 2_740_937_888,
        license: "Apache-2.0",
        blurb: "Best colour sense. Knows the most films, games and places. Alibaba Qwen.",
        temperature: 0.2,
        retired: false,
    },
    ModelSpec {
        id: "qwen3.5-2b",
        name: "Qwen3.5 2B",
        file: "Qwen3.5-2B-Q4_K_M.gguf",
        url: "https://huggingface.co/unsloth/Qwen3.5-2B-GGUF/resolve/main/Qwen3.5-2B-Q4_K_M.gguf",
        sha256: "aaf42c8b7c3cab2bf3d69c355048d4a0ee9973d48f16c731c0520ee914699223",
        bytes: 1_280_835_840,
        license: "Apache-2.0",
        blurb: "Nearly as good at half the size. Quicker on a CPU. Alibaba Qwen.",
        temperature: 0.2,
        retired: false,
    },
    ModelSpec {
        id: "lfm3-1.7b",
        name: "LFM3 1.7B",
        file: "LFM3-1.7B-Q4_K_M.gguf",
        url: "https://huggingface.co/LiquidAI/LFM3-1.7B-GGUF/resolve/main/LFM3-1.7B-Q4_K_M.gguf",
        sha256: "3793e1e6e2266919d6e015287221b41100175a98eddaba92e111aed3b05c93c4",
        bytes: 1_099_627_808,
        license: "LFM Open License v1.0 (free under $10M revenue)",
        blurb: "Smallest and quickest to wake. Knows fewer names by itself. Liquid AI.",
        temperature: 0.1,
        retired: false,
    },
    ModelSpec {
        id: "lfm2.5-1.2b",
        name: "LFM2.5 1.2B (older)",
        file: "LFM2.5-1.2B-Instruct-Q4_K_M.gguf",
        url: "https://huggingface.co/LiquidAI/LFM2.5-1.2B-Instruct-GGUF/resolve/main/LFM2.5-1.2B-Instruct-Q4_K_M.gguf",
        sha256: "b1b3de114215d9507409a662a501a631095a479a419584e8a2ded6304b19b4f5",
        bytes: 730_895_168,
        license: "LFM Open License v1.0 (free under $10M revenue)",
        blurb: "Replaced by LFM3 1.7B.",
        temperature: 0.2,
        retired: true,
    },
    ModelSpec {
        id: "qwen3.5-0.8b",
        name: "Qwen3.5 0.8B (older)",
        file: "Qwen3.5-0.8B-Q4_K_M.gguf",
        url: "https://huggingface.co/unsloth/Qwen3.5-0.8B-GGUF/resolve/main/Qwen3.5-0.8B-Q4_K_M.gguf",
        sha256: "bd258782e35f7f458f8aced1adc053e6e92e89bc735ba3be89d38a06121dc517",
        bytes: 532_517_120,
        license: "Apache-2.0",
        blurb: "Replaced by Qwen3.5 2B.",
        temperature: 0.2,
        retired: true,
    },
];

#[derive(Debug, Clone, Copy)]
pub struct RuntimeSpec {
    pub tag: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub bytes: u64,
}

/// llama.cpp server, Vulkan build (runs on NVIDIA/AMD/Intel GPUs, falls back to CPU).
pub const RUNTIME: RuntimeSpec = RuntimeSpec {
    tag: "b12321",
    url: "https://github.com/ggml-org/llama.cpp/releases/download/b12321/llama-b12321-bin-win-vulkan-x64.zip",
    sha256: "de3d073fa71d88f067be27700b7110cc0679d1643390348c9e83ee8d0a6f7ba2",
    bytes: 38_052_699,
};

pub fn models_dir() -> PathBuf {
    data_dir().join("models")
}

pub fn runtime_dir() -> PathBuf {
    data_dir().join("runtime").join(RUNTIME.tag)
}

pub fn model_path(m: &ModelSpec) -> PathBuf {
    models_dir().join(m.file)
}

pub fn model_installed(m: &ModelSpec) -> bool {
    model_path(m).metadata().map(|md| md.len() > 1_000_000).unwrap_or(false)
}

pub fn server_exe() -> Option<PathBuf> {
    let dir = runtime_dir();
    let direct = dir.join("llama-server.exe");
    if direct.exists() {
        return Some(direct);
    }
    // some releases nest files in a folder
    std::fs::read_dir(&dir).ok()?.flatten().map(|e| e.path().join("llama-server.exe")).find(|p| p.exists())
}

pub fn find(id: &str) -> Option<&'static ModelSpec> {
    MODELS.iter().find(|m| m.id == id)
}

use crate::{Frame, MonitorInfo};
use anyhow::{anyhow, bail, Context, Result};
use windows::core::Interface;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;

pub fn monitors() -> Result<Vec<MonitorInfo>> {
    let mut out = Vec::new();
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
        let mut ai = 0;
        while let Ok(adapter) = factory.EnumAdapters1(ai) {
            let mut oi = 0;
            while let Ok(output) = adapter.EnumOutputs(oi) {
                let d = output.GetDesc()?;
                if d.AttachedToDesktop.as_bool() {
                    let end = d.DeviceName.iter().position(|c| *c == 0).unwrap_or(d.DeviceName.len());
                    let r = d.DesktopCoordinates;
                    out.push(MonitorInfo {
                        adapter: ai,
                        output: oi,
                        name: String::from_utf16_lossy(&d.DeviceName[..end]),
                        left: r.left,
                        top: r.top,
                        width: r.right - r.left,
                        height: r.bottom - r.top,
                    });
                }
                oi += 1;
            }
            ai += 1;
        }
    }
    Ok(out)
}

struct MipChain {
    tex: ID3D11Texture2D,
    srv: ID3D11ShaderResourceView,
    staging: ID3D11Texture2D,
    src_w: u32,
    src_h: u32,
    level: u32,
    out_w: u32,
    out_h: u32,
}

pub struct Capturer {
    device: ID3D11Device,
    ctx: ID3D11DeviceContext,
    dup: IDXGIOutputDuplication,
    chain: Option<MipChain>,
    target_width: u32,
    pub monitor: MonitorInfo,
}

pub enum Grab {
    Frame(Frame),
    Unchanged,
}

impl Capturer {
    /// `target_width`: frames are downscaled on the GPU to the smallest mip level at least this wide.
    pub fn new(monitor: &MonitorInfo, target_width: u32) -> Result<Self> {
        unsafe {
            let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
            let adapter = factory.EnumAdapters1(monitor.adapter).context("adapter gone")?;
            let output = adapter.EnumOutputs(monitor.output).context("monitor gone")?;

            let mut device = None;
            let mut ctx = None;
            D3D11CreateDevice(
                &adapter,
                D3D_DRIVER_TYPE_UNKNOWN,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut ctx),
            )
            .context("D3D11CreateDevice")?;
            let device = device.ok_or_else(|| anyhow!("no D3D device"))?;
            let ctx = ctx.ok_or_else(|| anyhow!("no D3D context"))?;

            // DuplicateOutput1 with BGRA8 makes Windows convert HDR output; older outputs fall back
            // to DuplicateOutput.
            let dup = match output.cast::<IDXGIOutput5>() {
                Ok(o5) => o5.DuplicateOutput1(&device, 0, &[DXGI_FORMAT_B8G8R8A8_UNORM]),
                Err(e) => Err(e),
            };
            let dup = match dup {
                Ok(d) => d,
                Err(_) => output.cast::<IDXGIOutput1>()?.DuplicateOutput(&device).context(
                    "screen capture blocked (another app may be capturing, or the screen is protected)",
                )?,
            };
            Ok(Self { device, ctx, dup, chain: None, target_width: target_width.max(32), monitor: monitor.clone() })
        }
    }

    fn ensure_chain(&mut self, w: u32, h: u32, format: DXGI_FORMAT) -> Result<()> {
        if let Some(c) = &self.chain {
            if c.src_w == w && c.src_h == h {
                return Ok(());
            }
        }
        if format != DXGI_FORMAT_B8G8R8A8_UNORM {
            bail!("unsupported desktop format {format:?}");
        }
        let mut level = 0;
        while (w >> (level + 1)) >= self.target_width && (h >> (level + 1)) >= 16 {
            level += 1;
        }
        let (out_w, out_h) = ((w >> level).max(1), (h >> level).max(1));
        unsafe {
            let desc = D3D11_TEXTURE2D_DESC {
                Width: w,
                Height: h,
                MipLevels: level + 1,
                ArraySize: 1,
                Format: format,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: (D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_RENDER_TARGET.0) as u32,
                CPUAccessFlags: 0,
                MiscFlags: D3D11_RESOURCE_MISC_GENERATE_MIPS.0 as u32,
            };
            let mut tex = None;
            self.device.CreateTexture2D(&desc, None, Some(&mut tex))?;
            let tex = tex.unwrap();
            let mut srv = None;
            self.device.CreateShaderResourceView(&tex, None, Some(&mut srv))?;
            let staging_desc = D3D11_TEXTURE2D_DESC {
                Width: out_w,
                Height: out_h,
                MipLevels: 1,
                ArraySize: 1,
                Format: format,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                Usage: D3D11_USAGE_STAGING,
                BindFlags: 0,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                MiscFlags: 0,
            };
            let mut staging = None;
            self.device.CreateTexture2D(&staging_desc, None, Some(&mut staging))?;
            self.chain = Some(MipChain {
                tex,
                srv: srv.unwrap(),
                staging: staging.unwrap(),
                src_w: w,
                src_h: h,
                level,
                out_w,
                out_h,
            });
        }
        Ok(())
    }

    /// Waits up to `timeout_ms` for a new desktop frame.
    pub fn grab(&mut self, timeout_ms: u32) -> Result<Grab> {
        unsafe {
            let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
            let mut res: Option<IDXGIResource> = None;
            if let Err(e) = self.dup.AcquireNextFrame(timeout_ms, &mut info, &mut res) {
                if e.code() == DXGI_ERROR_WAIT_TIMEOUT {
                    return Ok(Grab::Unchanged);
                }
                if e.code() == DXGI_ERROR_ACCESS_LOST {
                    bail!("capture lost (display mode changed or a secure screen is up)");
                }
                return Err(e.into());
            }
            let result = (|| -> Result<Grab> {
                if info.LastPresentTime == 0 {
                    return Ok(Grab::Unchanged); // only the mouse moved
                }
                let tex: ID3D11Texture2D = res.as_ref().ok_or_else(|| anyhow!("no frame"))?.cast()?;
                let mut d = D3D11_TEXTURE2D_DESC::default();
                tex.GetDesc(&mut d);
                self.ensure_chain(d.Width, d.Height, d.Format)?;
                let c = self.chain.as_ref().unwrap();
                self.ctx.CopySubresourceRegion(&c.tex, 0, 0, 0, 0, &tex, 0, None);
                self.ctx.GenerateMips(&c.srv);
                self.ctx.CopySubresourceRegion(&c.staging, 0, 0, 0, 0, &c.tex, c.level, None);
                let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                self.ctx.Map(&c.staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
                let row = (c.out_w * 4) as usize;
                let mut bgra = vec![0u8; row * c.out_h as usize];
                let src = mapped.pData as *const u8;
                for y in 0..c.out_h as usize {
                    std::ptr::copy_nonoverlapping(src.add(y * mapped.RowPitch as usize), bgra.as_mut_ptr().add(y * row), row);
                }
                self.ctx.Unmap(&c.staging, 0);
                Ok(Grab::Frame(Frame { width: c.out_w, height: c.out_h, bgra }))
            })();
            let _ = self.dup.ReleaseFrame();
            result
        }
    }
}
